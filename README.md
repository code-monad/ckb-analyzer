# CKB Node Probe

The purpose of CKB Node Probe is to facilitate observation of the CKB network.

CKB Node Probe acts as a metrics agent and stores the data into [Timescaledb](https://docs.timescale.com/), then exposed an API using [Marci](https://github.com/code-monad/Marci.git).

## Getting Started

### Quick deployment
1. Install [Docker](https://docs.docker.com/get-docker/) and [Docker Compose](https://docs.docker.com/compose/install/).
2. Clone this repo and enter the directory: `git clone https://github.com/cryptape/ckb-node-probe && cd ckb-node-probe && git submodule update --init --recursive`
3. Modify the deployment file [docker-compose.yaml](./docker-compose.yaml).(Or you can keep the default contents), enter you [ipinfo.io token](https://ipinfo.io/account/token) .
4. Run `docker-compose up -d` to start the all services.
5. (*Notice to run after migration with exist db*)Run `docker exec -it ckb-analyzer-postgresql "/usr/bin/import_ip_data"` to download & import ip data(Only the first time, after this it will use a cronjob).

Now you can visit http://localhost:1800 to see the dashboards.

### Migration

We restructured the DB, you may need to migrate your data to the new schema.

1. Stop ckb-analyzer service: `docker-compose stop ckb-analyzer`

2. Run this command to init new schema: `docker-compose exec -it postgresql psql -U postgres -d ckb -c "CREATE SCHEMA IF NOT EXISTS common_info; CREATE TABLE IF NOT EXISTS common_info.ip_info (ip_range_start TEXT NOT NULL, ip_range_end TEXT NOT NULL, country_code TEXT NOT NULL, state1 TEXT, state2 TEXT, city TEXT, postcode TEXT, latitude NUMERIC(9, 6), longitude NUMERIC(9, 6), timezone TEXT);"`

3. Rebuild the image and start the service: `docker-compose stop postgresql && docker-compose build postgresql && docker-compose up -d`

Make sure you also updated the other services, if you are not sure, follow these steps:
1. run a `docker-compose down --remove-orphans`
2. then `docker-compose build`
3. and then `docker-compose up -d` to start all services.

**For a detailed deployment guide, follow the bellow parts**

### Setup TimescaleDB service
*NOTE: If you use the integrated docker-compose config to deploy db, this step will be automatically set-up, so you can skip it* 
```shell
# Assume you have a local db
$ psql "postgres://postgres:postgres@127.0.0.1" -f sql/schema.sql

# Modify db url, passwd users and run script to import ip infos db
bash sql/import_ip_data.sh
```

### Install CKB Node Probe

```shell
cargo install --path . -f
```

### Run CKB Node Probe

Mostly configurations are declared inside [`ckb-analyzer.toml`](./ckb-analyzer.toml). You can specify a config file with `--config`.
Modify the config file, fill you IPINFO_IO_TOKEN, and run the analyzer.
```shell
# NOTE: remember to modify your custom configuration after copy
ckb-analyzer --config config.toml 
```

### Run Marci

Marci is the frontend service of CKB Node Probe. You can find it in the submodule [frontend/Marci](./frontend/Marci)
```shell
cd frontend/Marci

cargo run -- --db-url "postgres://postgres:postgres@127.0.0.1:5432/ckb" --bind "0.0.0.0:1800"
```

---

License: MIT
