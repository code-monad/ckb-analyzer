# Pull in the latest TimescaleDB image
FROM timescale/timescaledb:latest-pg12

COPY schema.sql /docker-entrypoint-initdb.d/schema.sql
COPY init_db.sh /docker-entrypoint-initdb.d/002_custom_db_setup.sh
COPY import_ip_data.sh /usr/bin/import_ip_data
