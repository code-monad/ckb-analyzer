# CKBAnalyzer

The purpose of CKBAnalyzer is to facilitate observation of the CKB network.

CKBAnalyzer acts as a metrics agent and stores the data into [Timescaledb](https://docs.timescale.com/), then visualize using [Grafana](https://grafana.com/).

Visit the online dashboards at [https://ckbmonitor.bit.host/], and you can use the [maintained dashboards](https://github.com/keroro520/ckb-analyzer/tree/main/dashboards).

## Getting Started

### Setup TimescaleDB and Grafana services via docker-compose
*NOTE: If you use the integrated docker-compose config to deploy, this step will be automatically set-up* 
```shell
$ cp docker/.env.example docker/.env

$ source docker/.env && psql "postgres://$POSTGRES_USER:$POSTGRES_PASSWORD@127.0.0.1:${POSTGRES_PORT:-"5432"}" -f sql/schema.sql
```

### Install CKBAnalyzer

```shell
cargo install --path . -f
```

### Run CKBAnalyzer

Mostly configurations are declared inside [`confit.example.toml`](./config.example.toml). You can specify an config file with `--config`.

```shell
# NOTE: remember to modify your custom configuration after copy
cp config.example.toml config.toml
ckb-analyzer --config config.toml 
```

---

License: MIT
