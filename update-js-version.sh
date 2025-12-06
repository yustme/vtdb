#!/bin/bash

# Script to update the JavaScript and CSS version in index.html to bust browser cache
# Usage: ./update-js-version.sh
# This script updates the version query parameter for app.js and style.css

set -e

WEB_DIR="web"
INDEX_HTML="${WEB_DIR}/index.html"
VERSION_FILE="${WEB_DIR}/.js-version"

# Generate a new version based on timestamp
NEW_VERSION=$(date +%s)

# Write version to file
echo "$NEW_VERSION" > "$VERSION_FILE"

# Update index.html with the new version for both app.js and style.css
# Match any existing version number or {{VERSION}} placeholder
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    sed -i '' "s/app\.js?v=[0-9]*/app.js?v=${NEW_VERSION}/g" "$INDEX_HTML"
    sed -i '' "s/app\.js?v={{VERSION}}/app.js?v=${NEW_VERSION}/g" "$INDEX_HTML"
    sed -i '' "s/style\.css?v=[0-9]*/style.css?v=${NEW_VERSION}/g" "$INDEX_HTML"
    sed -i '' "s/style\.css?v={{VERSION}}/style.css?v=${NEW_VERSION}/g" "$INDEX_HTML"
else
    # Linux
    sed -i "s/app\.js?v=[0-9]*/app.js?v=${NEW_VERSION}/g" "$INDEX_HTML"
    sed -i "s/app\.js?v={{VERSION}}/app.js?v=${NEW_VERSION}/g" "$INDEX_HTML"
    sed -i "s/style\.css?v=[0-9]*/style.css?v=${NEW_VERSION}/g" "$INDEX_HTML"
    sed -i "s/style\.css?v={{VERSION}}/style.css?v=${NEW_VERSION}/g" "$INDEX_HTML"
fi

echo "Updated app.js and style.css version to: ${NEW_VERSION}"
echo "Browser cache will be invalidated on next page load."

