# Hatcheck Sessions for VS Code

Resume sessions, inspect cached plan usage and open local usage insights through the stable
`hatcheck` JSON interface. There are no runtime npm dependencies and no background process.

Run an Extension Development Host from the repository root:

```sh
code --extensionDevelopmentPath="$PWD/integrations/vscode-hatcheck"
```

Commands: **Hatcheck: Resume a Session**, **Hatcheck: Cached Plan Usage** and **Hatcheck: Local
Usage Insights**. The `hatcheck.path` setting names the executable; `HATCHECK_BIN` overrides it.
`npm test` runs the built-in Node tests.
