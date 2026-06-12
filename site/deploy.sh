#!/usr/bin/env bash
# Build and deploy loadr.io: marketing site + docs → S3 → CloudFront invalidation.
# Usage: AWS_PROFILE=personal ./site/deploy.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$ROOT/site/dist"
BUCKET="loadr.io"
DISTRIBUTION_ID="${DISTRIBUTION_ID:-E1O86EYW71WS3}"
AWS_PROFILE="${AWS_PROFILE:-personal}"

echo "==> building CSS"
(cd "$ROOT/site" && npx @tailwindcss/cli -i src/input.css -o assets/site.css --minify)

echo "==> building docs"
mdbook build "$ROOT/docs"

echo "==> assembling dist"
rm -rf "$DIST"
mkdir -p "$DIST/assets" "$DIST/docs"
cp "$ROOT/site/index.html" "$ROOT/site/404.html" "$DIST/"
mkdir -p "$DIST/demos"
cp "$ROOT/site/demos.html" "$DIST/demos/index.html"
cp "$ROOT/site/assets/site.css" "$ROOT/site/assets/site.js" "$ROOT/site/assets/favicon.svg" "$DIST/assets/"
cp -r "$ROOT/docs/book/." "$DIST/docs/"
if [ -d "$ROOT/site/videos/out" ]; then
  mkdir -p "$DIST/videos"
  cp "$ROOT/site/videos/out/"*.mp4 "$ROOT/site/videos/out/"*.jpg "$DIST/videos/" 2>/dev/null || true
fi

echo "==> syncing to s3://$BUCKET"
# Long-lived cache for static assets…
aws-vault exec "$AWS_PROFILE" -- aws s3 sync "$DIST" "s3://$BUCKET" \
  --delete \
  --exclude "*.html" \
  --cache-control "public, max-age=86400" \
  --quiet
# …short cache for HTML so deploys propagate fast.
aws-vault exec "$AWS_PROFILE" -- aws s3 sync "$DIST" "s3://$BUCKET" \
  --exclude "*" --include "*.html" \
  --cache-control "public, max-age=300, must-revalidate" \
  --content-type "text/html; charset=utf-8" \
  --quiet

echo "==> invalidating CloudFront ($DISTRIBUTION_ID)"
aws-vault exec "$AWS_PROFILE" -- aws cloudfront create-invalidation \
  --distribution-id "$DISTRIBUTION_ID" \
  --paths "/*" \
  --query "Invalidation.Id" --output text

echo "==> done: https://loadr.io"
