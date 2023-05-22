# Pull in the latest TimescaleDB image
FROM timescale/timescaledb:latest-pg12

COPY schema.sql /docker-entrypoint-initdb.d/schema.sql
COPY init_db.sh /docker-entrypoint-initdb.d/002_custom_db_setup.sh
COPY import_ip_data.sh /usr/bin/import_ip_data
#Crontab for grabing ip data
RUN echo "0 0 1 * * root /usr/bin/import_ip_data >> /var/log/import_ip_data.log 2>&1" >> /etc/crontabs/root
RUN chmod 0644 /etc/crontabs/ip_data_import
RUN crontab /etc/crontabs/ip_data_import
