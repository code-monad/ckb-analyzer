# CKBAnalyzer

The purpose of CKBAnalyzer is to facilitate observation of the CKB network.

CKBAnalyzer acts as a metrics agent and stores the data into [Timescaledb](https://docs.timescale.com/), then visualize using [Grafana](https://grafana.com/).

Visit the online dashboards at [https://ckbmonitor.bit.host/], and you can use the [maintained dashboards](https://github.com/keroro520/ckb-analyzer/tree/main/dashboards).

## Getting Started

### Quick deployment
1. Install [Docker](https://docs.docker.com/get-docker/) and [Docker Compose](https://docs.docker.com/compose/install/).
2. Clone this repo and enter the directory: `git clone https://github.com/code-monad/ckb-analyzer`
3. Modify the configuration file [docker/ckb-analyzer.toml](docker/ckb-analyzer.toml).(Or you can keep the default contents)
4. Run `docker-compose -f docker/collector.yaml up -d` to start the discovery services.
5. Run `docker-compose -f docker/monitor.yaml up -d` to start the grafana service.

Now you can visit http://localhost:3000 to see the dashboards.

**For a detailed deployment guide, follow the bellow parts**

### Setup TimescaleDB and Grafana services via docker-compose
*NOTE: If you use the integrated docker-compose config to deploy db, this step will be automatically set-up, so you can skip it* 
```shell
$ cp docker/.env.example docker/.env

$ source docker/.env && psql "postgres://$POSTGRES_USER:$POSTGRES_PASSWORD@127.0.0.1:${POSTGRES_PORT:-"5432"}" -f sql/schema.sql
```

### Install CKBAnalyzer

```shell
cargo install --path . -f
```

### Run CKBAnalyzer

Mostly configurations are declared inside [`confit.example.toml`](./config.example.toml). You can specify a config file with `--config`.

```shell
# NOTE: remember to modify your custom configuration after copy
cp config.example.toml config.toml
ckb-analyzer --config config.toml 
```

---

License: MIT
