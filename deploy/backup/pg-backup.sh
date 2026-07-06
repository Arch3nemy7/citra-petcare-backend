#!/usr/bin/env bash
# Nightly logical backup of the citra-petcare database, uploaded to the same
# OCI Object Storage bucket used for files (S3 Compatibility API).
#
# Requirements on the VPS:
#   sudo apt-get install -y awscli
#   aws configure --profile oci-backup     # OCI Customer Secret Key id/secret
#
# Configure via /etc/citra-petcare-backup.env (see variables below), then:
#   sudo crontab -e
#   15 3 * * * /opt/citra-petcare/deploy/backup/pg-backup.sh >> /var/log/petcare-backup.log 2>&1
#
# (03:15 server time; set the server TZ to Asia/Jakarta or adjust.)

set -euo pipefail

# ---- configuration ----------------------------------------------------------
ENV_FILE=${ENV_FILE:-/etc/citra-petcare-backup.env}
# shellcheck disable=SC1090
[[ -f "$ENV_FILE" ]] && source "$ENV_FILE"

COMPOSE_DIR=${COMPOSE_DIR:-/opt/citra-petcare}            # where docker-compose.yml lives
DB_SERVICE=${DB_SERVICE:-db}
DB_USER=${DB_USER:-petcare}
DB_NAME=${DB_NAME:-petcare}
S3_BUCKET=${BACKUP_S3_BUCKET:?set BACKUP_S3_BUCKET in $ENV_FILE}
S3_PREFIX=${BACKUP_S3_PREFIX:-backups}
S3_ENDPOINT=${BACKUP_S3_ENDPOINT:?set BACKUP_S3_ENDPOINT (https://{namespace}.compat.objectstorage.{region}.oraclecloud.com)}
AWS_PROFILE=${BACKUP_AWS_PROFILE:-oci-backup}
KEEP_DAYS=${BACKUP_KEEP_DAYS:-30}
# ------------------------------------------------------------------------------

STAMP=$(date +%Y%m%d-%H%M%S)
FILE="petcare-${STAMP}.sql.gz"
TMP="$(mktemp -d)/${FILE}"
trap 'rm -rf "$(dirname "$TMP")"' EXIT

echo "[$(date -Is)] dumping ${DB_NAME}…"
docker compose --project-directory "$COMPOSE_DIR" exec -T "$DB_SERVICE" \
    pg_dump -U "$DB_USER" --no-owner --format=plain "$DB_NAME" | gzip -9 > "$TMP"

SIZE=$(du -h "$TMP" | cut -f1)
echo "[$(date -Is)] uploading ${FILE} (${SIZE}) to s3://${S3_BUCKET}/${S3_PREFIX}/…"
aws --profile "$AWS_PROFILE" --endpoint-url "$S3_ENDPOINT" \
    s3 cp "$TMP" "s3://${S3_BUCKET}/${S3_PREFIX}/${FILE}" --only-show-errors

# retention: delete remote dumps older than KEEP_DAYS
echo "[$(date -Is)] pruning backups older than ${KEEP_DAYS} days…"
CUTOFF=$(date -d "-${KEEP_DAYS} days" +%s)
aws --profile "$AWS_PROFILE" --endpoint-url "$S3_ENDPOINT" \
    s3 ls "s3://${S3_BUCKET}/${S3_PREFIX}/" | while read -r day time _size key; do
    [[ "$key" == petcare-*.sql.gz ]] || continue
    if [[ $(date -d "$day $time" +%s) -lt $CUTOFF ]]; then
        aws --profile "$AWS_PROFILE" --endpoint-url "$S3_ENDPOINT" \
            s3 rm "s3://${S3_BUCKET}/${S3_PREFIX}/${key}" --only-show-errors
        echo "  pruned ${key}"
    fi
done

echo "[$(date -Is)] backup complete: ${FILE}"
