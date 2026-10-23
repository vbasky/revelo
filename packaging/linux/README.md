# Debian and RPM packages

`packaging/nfpm.yaml` plus `pack.sh` build `.deb` and `.rpm` from a release `revelo` binary (BSD-2-Clause default features).

```fish
just pkg-deb-rpm
# or: bash packaging/linux/pack.sh
```

Output is under `dist/`:

- `revelo_<version>-1_amd64.deb` / `revelo_<version>-1_arm64.deb`
- `revelo-<version>-1.x86_64.rpm` / `revelo-<version>-1.aarch64.rpm`

GitHub Actions (`.github/workflows/deb-rpm.yml`) builds the x86_64 pair on `v*` tags and attaches them to the GitHub Release.

```fish
sudo apt install ./dist/revelo_0.5.6-1_arm64.deb
sudo rpm -Uvh dist/revelo-0.5.6-1.aarch64.rpm
```
