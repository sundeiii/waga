export interface Crumb {
  label: string;
  path: string;
}

/** "/home/u/docs" -> / · home · u · docs */
export function remoteCrumbs(path: string): Crumb[] {
  const out: Crumb[] = [{ label: "/", path: "/" }];
  let acc = "";
  for (const part of path.split("/").filter(Boolean)) {
    acc += `/${part}`;
    out.push({ label: part, path: acc });
  }
  return out;
}

/** "C:\Users\me" -> This PC · C: · Users · me ("" = drive list) */
export function localCrumbs(path: string): Crumb[] {
  const out: Crumb[] = [{ label: "This PC", path: "" }];
  if (!path) return out;
  const parts = path.split("\\").filter(Boolean);
  let acc = "";
  parts.forEach((part, i) => {
    acc = i === 0 ? `${part}\\` : acc.endsWith("\\") ? `${acc}${part}` : `${acc}\\${part}`;
    out.push({ label: part, path: acc });
  });
  return out;
}

/** Remote names are untrusted: make control characters visible instead of rendering them. */
export function displayName(name: string): string {
  // eslint-disable-next-line no-control-regex
  return name.replace(/[\u0000-\u001f\u007f-\u009f]/g, "\uFFFD");
}
