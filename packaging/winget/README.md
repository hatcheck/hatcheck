# winget manifests

`cli/` holds the hatcheck CLI's (`hatcheck`, `hat`, `cl`, `cx`, `ck`, `claude-z` and `claude-k`),
package `hatcheck.Hatcheck.CLI`, which go to
[microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) as
`manifests/h/hatcheck/Hatcheck/CLI/<version>/`, the three files as they are here. The package
installs the commands from the release's Windows zip, made by cargo-dist (`v-release.yml`), as
portable commands. For each release: `PackageVersion` in all three, the zip's URL and SHA-256 in
the installer manifest (the release's `hatcheck-x86_64-pc-windows-msvc.zip.sha256`, upper case),
and the release notes link in the locale one; [docs/maintenance.md](../../docs/maintenance.md#releasing-the-cli)
has the steps, and `tests/test_packaging.py` checks the versions, the URL and the commands.

The three files beside this one are the desktop app's, package `hatcheck.Hatcheck`, for
`manifests/h/hatcheck/Hatcheck/<version>/`. They wait for the app's first release.
