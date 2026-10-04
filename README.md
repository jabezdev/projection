# Magi Projection

Software for a church worship team: a song library the team prepares during the week, and a projection app that puts it on screen on Saturday and Sunday.

The repo holds two separate projects that share one git history. Each has its own dependencies, lockfile, tooling and deploys, so you can work on and ship one without touching the other.

| Folder | What | Built with | Start here |
|---|---|---|---|
| [desktop/](desktop/) | Projection app the operator runs at the venue | Rust, Tauri, wgpu, React | [desktop/README.md](desktop/README.md) |
| [web/](web/) | Song library web app for the team: lyrics, service arrangements, roles | React PWA, Convex, Clerk | [web/README.md](web/README.md) |

How the two fit together: [ARCHITECTURE.md](ARCHITECTURE.md). What's done and what's next: [task.md](task.md).

## License

MIT
