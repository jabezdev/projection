'use strict';

const RELEASES = 'https://api.github.com/repos/jabezdev/projection/releases?per_page=30';

const megabytes = bytes => `${Math.round(bytes / 1048576)} MB`;

function pick(releases, tagPrefix, assetPattern) {
  for (const release of releases) {
    if (release.draft || release.prerelease || !release.tag_name.startsWith(tagPrefix)) continue;
    const asset = release.assets.find(a => assetPattern.test(a.name));
    if (asset) return { release, asset };
  }
  return null;
}

function show(name, found) {
  if (!found) return;
  const { release, asset } = found;
  const version = release.tag_name.replace(/^(obs-)?v/, '');
  const date = new Date(release.published_at).toLocaleDateString(undefined, { year: 'numeric', month: 'long', day: 'numeric' });
  document.querySelectorAll(`[data-download="${name}"]`).forEach(link => { link.href = asset.browser_download_url; });
  document.querySelectorAll(`[data-version="${name}"]`).forEach(el => { el.textContent = version; });
  document.querySelectorAll(`[data-version-line="${name}"]`).forEach(el => { el.textContent = `Version ${version} · ${megabytes(asset.size)} · ${date}`; });
}

fetch(RELEASES, { headers: { Accept: 'application/vnd.github+json' } })
  .then(response => (response.ok ? response.json() : []))
  .then(releases => {
    show('palto', pick(releases, 'v', /^PALTO_.*_x64-setup\.exe$/));
    show('obs', pick(releases, 'obs-v', /-windows-x64-setup\.exe$/));
  })
  .catch(() => {});
