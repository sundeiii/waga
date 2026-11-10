//! Import VMs from cloud providers as server profiles.
//!
//! * DigitalOcean / Hetzner Cloud / Linode: official REST APIs with a read
//!   token (kept only in memory, or in Credential Manager if the user asks).
//! * AWS / Azure: the user's own signed-in `aws` / `az` CLI with fixed
//!   arguments — yoru never sees those credentials.
//! Responses are untrusted: every string is length-limited and cleaned.

use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Provider {
    DigitalOcean,
    Hetzner,
    Linode,
    Aws,
    Azure,
}

impl Provider {
    pub fn key(&self) -> &'static str {
        match self {
            Provider::DigitalOcean => "digitalocean",
            Provider::Hetzner => "hetzner",
            Provider::Linode => "linode",
            Provider::Aws => "aws",
            Provider::Azure => "azure",
        }
    }
    pub fn uses_token(&self) -> bool {
        matches!(self, Provider::DigitalOcean | Provider::Hetzner | Provider::Linode)
    }
    pub fn token_account(&self) -> String {
        format!("cloud/{}/token", self.key())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CloudVm {
    pub provider: Provider,
    pub id: String,
    pub name: String,
    /// Best address to SSH to (public IPv4 > public IPv6 > private).
    pub host: Option<String>,
    pub private_ip: Option<String>,
    pub region: String,
    pub os: Option<String>,
    pub windows: bool,
    pub status: String,
    pub suggested_user: String,
    pub tags: Vec<String>,
}

fn s(v: &Value) -> String {
    clean(v.as_str().unwrap_or_default(), 120)
}

fn clean(x: &str, max: usize) -> String {
    x.chars().filter(|c| !c.is_control()).take(max).collect::<String>().trim().to_string()
}

fn ip_ok(ip: &str) -> Option<String> {
    let ip = ip.trim();
    (ip.parse::<std::net::IpAddr>().is_ok()).then(|| ip.to_string())
}

// ---------------------------------------------------------------- parsers (pure, tested)

pub fn parse_digitalocean(v: &Value) -> Vec<CloudVm> {
    v["droplets"].as_array().into_iter().flatten().map(|d| {
        let nets4 = d["networks"]["v4"].as_array().cloned().unwrap_or_default();
        let pick = |t: &str| nets4.iter().find(|n| n["type"] == t).and_then(|n| n["ip_address"].as_str()).and_then(ip_ok);
        let v6 = d["networks"]["v6"].as_array().and_then(|a| a.iter().find(|n| n["type"] == "public")).and_then(|n| n["ip_address"].as_str()).and_then(ip_ok);
        let dist = s(&d["image"]["distribution"]);
        let os = [dist.clone(), s(&d["image"]["name"])].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ");
        CloudVm {
            provider: Provider::DigitalOcean,
            id: d["id"].to_string(),
            name: s(&d["name"]),
            host: pick("public").or(v6.clone()).or_else(|| pick("private")),
            private_ip: pick("private"),
            region: s(&d["region"]["slug"]),
            windows: false,
            os: (!os.is_empty()).then_some(os),
            status: s(&d["status"]),
            suggested_user: "root".into(),
            tags: d["tags"].as_array().into_iter().flatten().filter_map(|t| t.as_str()).map(|t| clean(t, 32)).collect(),
        }
    }).collect()
}

pub fn parse_hetzner(v: &Value) -> Vec<CloudVm> {
    v["servers"].as_array().into_iter().flatten().map(|d| {
        let v4 = d["public_net"]["ipv4"]["ip"].as_str().and_then(ip_ok);
        // Hetzner reports the /64 network ("2a01:4f8::/64"); the server is ::1.
        let v6 = d["public_net"]["ipv6"]["ip"].as_str().and_then(|n| n.split('/').next()).and_then(|p| ip_ok(&format!("{}::1", p.trim_end_matches(':'))));
        let private = d["private_net"].as_array().and_then(|a| a.first()).and_then(|n| n["ip"].as_str()).and_then(ip_ok);
        let os = [s(&d["image"]["os_flavor"]), s(&d["image"]["os_version"])].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ");
        CloudVm {
            provider: Provider::Hetzner,
            id: d["id"].to_string(),
            name: s(&d["name"]),
            host: v4.or(v6).or(private.clone()),
            private_ip: private,
            region: s(&d["datacenter"]["location"]["name"]),
            windows: false,
            os: (!os.is_empty()).then_some(os),
            status: s(&d["status"]),
            suggested_user: "root".into(),
            tags: d["labels"].as_object().map(|m| m.iter().map(|(k, v)| clean(&format!("{k}={}", v.as_str().unwrap_or("")), 32)).collect()).unwrap_or_default(),
        }
    }).collect()
}

pub fn parse_linode(v: &Value) -> Vec<CloudVm> {
    v["data"].as_array().into_iter().flatten().map(|d| {
        let ips: Vec<String> = d["ipv4"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).filter_map(ip_ok).collect();
        let is_private = |ip: &str| ip.starts_with("192.168.") || ip.starts_with("10.") || ip.starts_with("172.");
        let public = ips.iter().find(|ip| !is_private(ip)).cloned();
        let private = ips.iter().find(|ip| is_private(ip)).cloned();
        let v6 = d["ipv6"].as_str().and_then(|x| x.split('/').next()).and_then(ip_ok);
        let image = s(&d["image"]);
        CloudVm {
            provider: Provider::Linode,
            id: d["id"].to_string(),
            name: s(&d["label"]),
            host: public.or(v6).or(private.clone()),
            private_ip: private,
            region: s(&d["region"]),
            windows: false,
            os: (!image.is_empty()).then(|| image.trim_start_matches("linode/").to_string()),
            status: s(&d["status"]),
            suggested_user: "root".into(),
            tags: d["tags"].as_array().into_iter().flatten().filter_map(|t| t.as_str()).map(|t| clean(t, 32)).collect(),
        }
    }).collect()
}

pub fn parse_aws(v: &Value) -> Vec<CloudVm> {
    v["Reservations"].as_array().into_iter().flatten().flat_map(|r| r["Instances"].as_array().cloned().unwrap_or_default()).map(|d| {
        let tag = |k: &str| d["Tags"].as_array().and_then(|a| a.iter().find(|t| t["Key"] == k)).map(|t| s(&t["Value"]));
        let windows = d["Platform"].as_str().is_some_and(|p| p.eq_ignore_ascii_case("windows"));
        let public = d["PublicIpAddress"].as_str().and_then(ip_ok);
        let private = d["PrivateIpAddress"].as_str().and_then(ip_ok);
        let platform = s(&d["PlatformDetails"]);
        CloudVm {
            provider: Provider::Aws,
            id: s(&d["InstanceId"]),
            name: tag("Name").filter(|n| !n.is_empty()).unwrap_or_else(|| s(&d["InstanceId"])),
            host: public.or(private.clone()),
            private_ip: private,
            region: s(&d["Placement"]["AvailabilityZone"]),
            windows,
            os: (!platform.is_empty()).then_some(platform),
            status: s(&d["State"]["Name"]),
            // Amazon Linux default; Ubuntu AMIs use "ubuntu" — editable before import.
            suggested_user: if windows { "Administrator".into() } else { "ec2-user".into() },
            tags: d["Tags"].as_array().into_iter().flatten().filter(|t| t["Key"] != "Name").map(|t| clean(&format!("{}={}", s(&t["Key"]), s(&t["Value"])), 32)).take(10).collect(),
        }
    }).collect()
}

pub fn parse_azure(v: &Value) -> Vec<CloudVm> {
    v.as_array().into_iter().flatten().map(|d| {
        let first = |k: &str| d[k].as_str().and_then(|x| x.split(',').next()).and_then(ip_ok);
        let os_type = s(&d["storageProfile"]["osDisk"]["osType"]);
        let windows = os_type.eq_ignore_ascii_case("windows");
        let image = [s(&d["storageProfile"]["imageReference"]["offer"]), s(&d["storageProfile"]["imageReference"]["sku"])].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ");
        let user = s(&d["osProfile"]["adminUsername"]);
        CloudVm {
            provider: Provider::Azure,
            id: s(&d["id"]),
            name: s(&d["name"]),
            host: first("publicIps").or_else(|| first("privateIps")),
            private_ip: first("privateIps"),
            region: s(&d["location"]),
            windows,
            os: Some(if image.is_empty() { os_type } else { image }).filter(|x| !x.is_empty()),
            status: s(&d["powerState"]).trim_start_matches("VM ").to_string(),
            suggested_user: if user.is_empty() { "azureuser".into() } else { user },
            tags: [s(&d["resourceGroup"])].into_iter().filter(|x| !x.is_empty()).map(|g| format!("rg={}", g.to_lowercase())).collect(),
        }
    }).collect()
}

// ---------------------------------------------------------------- fetching

fn client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("yoru-ssh/1")
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Internal(e.to_string()))
}

async fn get_json(c: &reqwest::Client, url: &str, token: &str, provider: &str) -> AppResult<Value> {
    let r = c.get(url).bearer_auth(token).send().await.map_err(|e| AppError::Validation(format!("Could not reach {provider}: {e}")))?;
    match r.status().as_u16() {
        200 => r.json().await.map_err(|e| AppError::Validation(format!("Unexpected {provider} response: {e}"))),
        401 | 403 => Err(AppError::Validation(format!("{provider} rejected the token (check it has read access)"))),
        code => Err(AppError::Validation(format!("{provider} returned HTTP {code}"))),
    }
}

const MAX_PAGES: usize = 20;

async fn fetch_api(provider: Provider, token: &str) -> AppResult<Vec<CloudVm>> {
    let c = client()?;
    let mut out = Vec::new();
    match provider {
        Provider::DigitalOcean => {
            let mut url = "https://api.digitalocean.com/v2/droplets?per_page=200".to_string();
            for _ in 0..MAX_PAGES {
                let v = get_json(&c, &url, token, "DigitalOcean").await?;
                out.extend(parse_digitalocean(&v));
                match v["links"]["pages"]["next"].as_str() {
                    Some(n) if n.starts_with("https://api.digitalocean.com/") => url = n.to_string(),
                    _ => break,
                }
            }
        }
        Provider::Hetzner => {
            for page in 1..=MAX_PAGES {
                let v = get_json(&c, &format!("https://api.hetzner.cloud/v1/servers?per_page=50&page={page}"), token, "Hetzner Cloud").await?;
                out.extend(parse_hetzner(&v));
                if v["meta"]["pagination"]["next_page"].is_null() {
                    break;
                }
            }
        }
        Provider::Linode => {
            for page in 1..=MAX_PAGES {
                let v = get_json(&c, &format!("https://api.linode.com/v4/linode/instances?page={page}&page_size=100"), token, "Linode").await?;
                out.extend(parse_linode(&v));
                if v["page"].as_u64() >= v["pages"].as_u64() {
                    break;
                }
            }
        }
        _ => unreachable!(),
    }
    Ok(out)
}

/// Run the user's signed-in CLI with FIXED arguments (only a validated region varies).
async fn run_cli(provider: Provider, region: Option<&str>) -> AppResult<Value> {
    let mut args: Vec<String> = match provider {
        Provider::Aws => vec!["aws".into(), "ec2".into(), "describe-instances".into(), "--output".into(), "json".into()],
        Provider::Azure => vec!["az".into(), "vm".into(), "list".into(), "-d".into(), "--output".into(), "json".into()],
        _ => unreachable!(),
    };
    if let (Provider::Aws, Some(r)) = (provider, region.filter(|r| !r.is_empty())) {
        if !r.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') || r.len() > 30 {
            return Err(AppError::Validation("Invalid AWS region".into()));
        }
        args.push("--region".into());
        args.push(r.to_string());
    }
    // via cmd so az.cmd / aws.exe resolve from PATH like in a terminal
    let mut cmd = tokio::process::Command::new("cmd");
    cmd.arg("/C").args(&args).kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let name = if provider == Provider::Aws { "AWS CLI (aws)" } else { "Azure CLI (az)" };
    let out = tokio::time::timeout(Duration::from_secs(90), cmd.output())
        .await
        .map_err(|_| AppError::Validation(format!("{name} took too long")))?
        .map_err(|e| AppError::Validation(format!("Could not run {name}: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let hint = if err.contains("is not recognized") {
            format!("{name} is not installed")
        } else if err.to_lowercase().contains("login") || err.contains("credentials") || err.contains("az login") {
            format!("{name} is not signed in (run `{}` in a terminal)", if provider == Provider::Aws { "aws configure sso" } else { "az login" })
        } else {
            clean(&err, 300)
        };
        return Err(AppError::Validation(hint));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| AppError::Validation(format!("Unexpected {name} output: {e}")))
}

pub async fn list(provider: Provider, token: Option<&str>, region: Option<&str>) -> AppResult<Vec<CloudVm>> {
    let mut vms = if provider.uses_token() {
        let t = token.filter(|t| !t.trim().is_empty()).ok_or_else(|| AppError::Validation("An API token is required".into()))?;
        fetch_api(provider, t.trim()).await?
    } else {
        let v = run_cli(provider, region).await?;
        if provider == Provider::Aws { parse_aws(&v) } else { parse_azure(&v) }
    };
    vms.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(vms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn digitalocean() {
        let v = json!({"droplets":[{"id":123,"name":"web-1","status":"active","region":{"slug":"fra1"},
            "image":{"distribution":"Ubuntu","name":"24.04 (LTS) x64"},
            "networks":{"v4":[{"ip_address":"10.114.0.2","type":"private"},{"ip_address":"164.90.1.2","type":"public"}],"v6":[]},
            "tags":["prod","web"]}]});
        let r = parse_digitalocean(&v);
        assert_eq!(r[0].host.as_deref(), Some("164.90.1.2"));
        assert_eq!(r[0].private_ip.as_deref(), Some("10.114.0.2"));
        assert_eq!(r[0].os.as_deref(), Some("Ubuntu 24.04 (LTS) x64"));
        assert_eq!(r[0].tags, vec!["prod", "web"]);
        assert_eq!(r[0].suggested_user, "root");
    }

    #[test]
    fn hetzner_ipv6_only() {
        let v = json!({"servers":[{"id":7,"name":"cx22","status":"running","public_net":{"ipv4":null,"ipv6":{"ip":"2a01:4f8:c012:abcd::/64"}},
            "private_net":[],"datacenter":{"location":{"name":"nbg1"}},"image":{"os_flavor":"debian","os_version":"12"},"labels":{"env":"dev"}}],
            "meta":{"pagination":{"next_page":null}}});
        let r = parse_hetzner(&v);
        assert_eq!(r[0].host.as_deref(), Some("2a01:4f8:c012:abcd::1"));
        assert_eq!(r[0].os.as_deref(), Some("debian 12"));
        assert_eq!(r[0].tags, vec!["env=dev"]);
    }

    #[test]
    fn linode() {
        let v = json!({"data":[{"id":9,"label":"db","status":"running","region":"eu-central","ipv4":["192.168.130.4","139.144.1.1"],"ipv6":"2a01:7e01::f03c:94ff:fe00:1/128","image":"linode/ubuntu24.04","tags":[]}],"page":1,"pages":1});
        let r = parse_linode(&v);
        assert_eq!(r[0].host.as_deref(), Some("139.144.1.1"));
        assert_eq!(r[0].private_ip.as_deref(), Some("192.168.130.4"));
        assert_eq!(r[0].os.as_deref(), Some("ubuntu24.04"));
    }

    #[test]
    fn aws() {
        let v = json!({"Reservations":[{"Instances":[
            {"InstanceId":"i-0abc","PublicIpAddress":"3.120.0.9","PrivateIpAddress":"172.31.5.5","State":{"Name":"running"},
             "Placement":{"AvailabilityZone":"eu-central-1a"},"PlatformDetails":"Linux/UNIX","Tags":[{"Key":"Name","Value":"bastion"},{"Key":"team","Value":"ops"}]},
            {"InstanceId":"i-0win","PrivateIpAddress":"172.31.9.9","Platform":"windows","State":{"Name":"stopped"},"Placement":{"AvailabilityZone":"eu-central-1b"}}]}]});
        let r = parse_aws(&v);
        assert_eq!(r[0].name, "bastion");
        assert_eq!(r[0].host.as_deref(), Some("3.120.0.9"));
        assert_eq!(r[0].tags, vec!["team=ops"]);
        assert!(r[1].windows);
        assert_eq!(r[1].name, "i-0win", "falls back to the instance id");
        assert_eq!(r[1].host.as_deref(), Some("172.31.9.9"));
        assert_eq!(r[1].suggested_user, "Administrator");
    }

    #[test]
    fn azure_uses_real_admin_user() {
        let v = json!([{"id":"/subscriptions/x/vm1","name":"vm1","location":"westeurope","powerState":"VM running","publicIps":"20.1.2.3",
            "privateIps":"10.0.0.4","resourceGroup":"RG-Web","osProfile":{"adminUsername":"opsadmin"},
            "storageProfile":{"osDisk":{"osType":"Linux"},"imageReference":{"offer":"0001-com-ubuntu-server-jammy","sku":"22_04-lts"}}}]);
        let r = parse_azure(&v);
        assert_eq!(r[0].suggested_user, "opsadmin");
        assert_eq!(r[0].host.as_deref(), Some("20.1.2.3"));
        assert_eq!(r[0].status, "running");
        assert_eq!(r[0].tags, vec!["rg=rg-web"]);
    }

    #[test]
    fn hostile_values_are_cleaned_or_dropped() {
        let v = json!({"droplets":[{"id":1,"name":"evil\u{1b}[31mname","networks":{"v4":[{"ip_address":"1.2.3.4; rm -rf /","type":"public"}]}}]});
        let r = parse_digitalocean(&v);
        assert_eq!(r[0].name, "evil[31mname");
        assert_eq!(r[0].host, None, "invalid IPs are never used as hosts");
    }
}
