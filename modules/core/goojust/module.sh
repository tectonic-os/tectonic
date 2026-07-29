dnf5 install -y just fastfetch

source /ctx/lib/fetch-helpers.sh

fetch_extract "https://github.com/tectonic-os/goojust/releases/download/v${GOOJUST_VERSION}/goojust-v${GOOJUST_VERSION}-x86_64-linux-gnu.tar.gz" \
    "$GOOJUST_SHA256" /tmp
bash /tmp/install.sh --no-config
rm -rf /tmp/goojust /tmp/install.sh /tmp/scripts/
