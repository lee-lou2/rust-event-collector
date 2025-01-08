#!/bin/bash

DB_FILE="sqlite3.db"

# 데이터베이스 파일 확인 및 생성
if [ ! -f "$DB_FILE" ]; then
  echo "Database file does not exist. Creating..."
  sqlite3 "$DB_FILE" <<EOF
CREATE TABLE IF NOT EXISTS events (id INTEGER PRIMARY KEY AUTOINCREMENT, log TEXT);
EOF
  echo "Database initialized."
else
  echo "Database file already exists."
fi
