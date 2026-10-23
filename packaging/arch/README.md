# Arch Linux package

`PKGBUILD` builds the BSD-2-Clause `revelo` CLI (`revelo-cli` default features).

```fish
cd packaging/arch
makepkg -sf
sudo pacman -U revelo-*-$CARCH.pkg.tar.zst
```

GitHub Actions (`.github/workflows/arch-package.yml`) builds the `x86_64` package in an `archlinux:base-devel` container on each `v*` tag and attaches it to the GitHub Release. Dispatch the same workflow to attach a package to an existing tag.

## Publish to the AUR

The AUR is a Git remote per package. GitHub Releases are not the AUR. `revelo` is unused on the AUR as of this writing.

1. Create an [AUR account](https://aur.archlinux.org/register) and add an SSH public key under [My Account](https://aur.archlinux.org/account/).
2. Configure SSH:

   ```fish
   printf '%s\n' 'Host aur.archlinux.org' '  User aur' '  IdentityFile ~/.ssh/id_ed25519' >> ~/.ssh/config
   ```

3. Clone the empty package (first upload creates it on push):

   ```fish
   git clone ssh://aur@aur.archlinux.org/revelo.git
   cd revelo
   cp ~/Projects/revelo/packaging/arch/PKGBUILD .
   cp ~/Projects/revelo/packaging/arch/Cargo.lock .
   makepkg --printsrcinfo > .SRCINFO
   git add PKGBUILD Cargo.lock .SRCINFO
   git commit -m "revelo 0.5.6-1"
   git push origin master
   ```

4. Later version bumps: update `pkgver` / checksums (`updpkgsums`), regenerate `.SRCINFO`, commit, push. Do not force-push AUR `master`.

Only `PKGBUILD`, `.SRCINFO`, and extra source files (here `Cargo.lock`) belong in the AUR tree. Built `.pkg.tar.zst` files stay on GitHub Releases.

Optional CI: add repo secret `AUR_SSH_PRIVATE_KEY` (the AUR-registered key) and a job that clones `ssh://aur@aur.archlinux.org/revelo.git` and pushes the same three files. Do not commit that key.
