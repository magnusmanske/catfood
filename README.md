# CatFood

RSS feeds of the latest files (or pages) in Wikimedia categories, and of a
user's latest uploads: <https://catfood.toolforge.org>

| Path | Feed |
|------|------|
| `/` (also `/catfood.php`, `/index.php`) | `category` (with `depth`, `namespace`) or `user`, on any wiki (`language`, `project`); `size`, `last` |
| `/commons_image_feed.php` | Random images from a Commons `category`, for screensavers; `max_images` |
| `/healthz` | Liveness check |

Without parameters, each feed shows a form.

## Development

```sh
cargo test
cargo run   # needs replica credentials, see below
```

Database credentials are found by the [`toolforge`](https://docs.rs/toolforge)
crate: the `TOOL_REPLICA_*` environment variables on Toolforge, or
`replica.my.cnf` with `local='true'` plus `toolforge-tunnel` locally. The server
listens on `$PORT` (default 8000); set `RUST_LOG` for log verbosity.

## Deployment (Toolforge build service)

```sh
become catfood
toolforge build start <public git URL>
toolforge webservice buildservice start --mount=none
```

To make a plain `toolforge webservice start` work, put this into
`$HOME/service.template`:

```yaml
type: buildservice
mount: none
```

## License

GPL 2.0 or later, see `COPYING`.
