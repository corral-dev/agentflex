# agentflex

Animated, shareable card of your coding-agent week. Zero install:

```sh
npx agentflex
```

The `agentflex` package is a small Node launcher. The real binary ships in
per-platform optional packages (`agentflex-darwin-arm64`,
`agentflex-darwin-x64`, `agentflex-linux-x64`, `agentflex-linux-arm64`,
`agentflex-win32-x64`, `agentflex-win32-arm64`), each declaring `os` and
`cpu` so your package manager installs only the matching one. No
postinstall scripts are used, so it works with npm, pnpm and Bun.

Supported platforms: macOS (arm64, x64), Linux (x64, arm64),
Windows (x64, arm64). Requires Node.js 18+.
