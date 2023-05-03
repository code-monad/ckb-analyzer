# CKBAnalyzer

The purpose of CKBAnalyzer is to facilitate observation of the CKB network.

CKBAnalyzer acts as a metrics agent and stores the data into [Timescaledb](https://docs.timescale.com/), then visualize using [Marci](https://github.com/code-monad/Marci.git).

Visit the online dashboards at [https://nodes.ckb.dev/], and you can [deploy an on-premise](#quick-deployment) one to visit locally.

## Getting Started

### Quick deployment
1. Install [Docker](https://docs.docker.com/get-docker/) and [Docker Compose](https://docs.docker.com/compose/install/).
2. Clone this repo and enter the directory: `git clone https://github.com/code-monad/ckb-analyzer && cd ckb-analyzer && git submodule update --init --recursive`
3. Modify the configuration file [ckb-analyzer.toml](./ckb-analyzer.toml).(Or you can keep the default contents), enter you [ipinfo.io token](https://ipinfo.io/account/token) .
4. Run `docker-compose up -d` to start the all services.

Now you can visit http://localhost:1800 to see the dashboards.

**For a detailed deployment guide, follow the bellow parts**

### Setup TimescaleDB service
*NOTE: If you use the integrated docker-compose config to deploy db, this step will be automatically set-up, so you can skip it* 
```shell
# Assume you have a local db
$ psql "postgres://postgres:postgres@127.0.0.1" -f sql/schema.sql
```

### Install CKBAnalyzer

```shell
cargo install --path . -f
```

### Run CKBAnalyzer

Mostly configurations are declared inside [`ckb-analyzer.toml`](./ckb-analyzer.toml). You can specify a config file with `--config`.
Modify the config file, fill you IPINFO_IO_TOKEN, and run the analyzer.
```shell
# NOTE: remember to modify your custom configuration after copy
ckb-analyzer --config config.toml 
```

### Run Marci

Marci is the frontend service of CKBAnalyzer. You can find it in the submodule [frontend/Marci](./frontend/Marci)
```shell
cd frontend/Marci

cargo run -- --db-url "postgres://postgres:postgres@127.0.0.1:5432/ckb" --bind "0.0.0.0:1800"
```

---

License: MIT
