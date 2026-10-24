# CatFood

RSS feeds of the latest files (or pages) in Wikimedia categories, and of a
user's latest uploads: <https://catfood.toolforge.org>

| Path | Feed |
|------|------|
| `/catfood` (also `/`) | `category` (with `depth`, `namespace`) or `user`, on any wiki (`language`, `project`); `size`, `last` |
| `/commons_image_feed` | Random images from a Commons `category`, for screensavers; `max_images` |
| `/healthz` | `OK` if a database connection is available, else 503 |

Without parameters, each feed shows a form. The PHP version's paths
(`/catfood.php`, `/index.php`, `/commons_image_feed.php`) redirect permanently
(301) to these, keeping the query string.

## Development

```sh
cargo test
cargo run   # needs replica credentials, see below
```

Database credentials are found by the [`toolforge`](https://docs.rs/toolforge)
crate: the `TOOL_REPLICA_*` environment variables on Toolforge, or
`replica.my.cnf` with `local='true'` plus `toolforge-tunnel` locally. The server
listens on `$PORT` (default 8000); set `RUST_LOG` for log verbosity.

With `$DASHBOARD_TOKEN` set, health, request metrics, and the number of feeds
served (`feeds_served_total`) are pushed to the
[service dashboard](https://magnustools.toolforge.org/buggregator/#/dashboard);
without it, telemetry is off.

## Deployment (Toolforge build service)

```sh
become catfood
toolforge envvars create DASHBOARD_TOKEN   # once, for telemetry
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
