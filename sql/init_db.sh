#!/usr/bin/env bash
echo "Init db!"
psql -U postgres -f /docker-entrypoint-initdb.d/schema.sql
