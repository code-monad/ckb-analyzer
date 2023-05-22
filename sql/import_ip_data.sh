#!/bin/bash

# Step 1: Download and unzip the files
download_file() {
  url=$1
  file_name=$2

  if [ -n "$DOWNLOAD_PROXY" ]; then
    echo "Using proxy: $DOWNLOAD_PROXY"
    HTTPS_PROXY=$DOWNLOAD_PROXY wget --proxy="$DOWNLOAD_PROXY" -O "$file_name" "$url"
  else
    wget -O "$file_name" "$url"
  fi

  if [ $? -ne 0 ]; then
    echo "Error downloading $file_name. Exiting..."
    rm "$file_name"
    exit 1
  fi
}
cd /tmp
download_file "https://github.com/sapics/ip-location-db/raw/master/geolite2-city/geolite2-city-ipv4.csv.gz" "geolite2-city-ipv4.csv.gz"
download_file "https://github.com/sapics/ip-location-db/raw/master/geolite2-city/geolite2-city-ipv6.csv.gz" "geolite2-city-ipv6.csv.gz"

gunzip geolite2-city-ipv4.csv.gz
gunzip geolite2-city-ipv6.csv.gz

# Step 2: Insert data into PostgreSQL
PGUSER="postgres"
PGDATABASE="ckb"

# Function to insert a CSV file into PostgreSQL
insert_csv_to_db() {
  csv_file=$1
  table_name=$2
  schema_name=$3

  psql -U "$PGUSER" -d "$PGDATABASE" -c "COPY $schema_name.$table_name FROM '$csv_file' DELIMITER ',' CSV;"
}

# Insert IPv4 data
insert_csv_to_db "geolite2-city-ipv4.csv" "ip_info" "common_info"

# Insert IPv6 data
insert_csv_to_db "geolite2-city-ipv6.csv" "ip_info" "common_info"

# Clean up files
rm geolite2-city-ipv4.csv
rm geolite2-city-ipv6.csv

echo "Data import completed successfully."
