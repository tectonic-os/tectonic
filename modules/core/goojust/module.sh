dnf5 install -y just fastfetch

source /ctx/lib/fetch-helpers.sh

fetch_extract "$ASSET_GOOJUST_URL" "$ASSET_GOOJUST_SHA256" /tmp
bash /tmp/install.sh --no-config
rm -rf /tmp/goojust /tmp/install.sh /tmp/scripts/
