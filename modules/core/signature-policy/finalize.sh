if [ -z "${IMAGE_REGISTRY:-}" ]; then
    echo "signature-policy: no registry namespace to scope the policy to;" \
        "this image will not verify its own updates" >&2
else
    mkdir -p /etc/containers/registries.d
    cat > /etc/containers/registries.d/10-sigstore.yaml << EOF
docker:
  ${IMAGE_REGISTRY}:
    use-sigstore-attachments: true
EOF

    python3 << 'PYEOF'
import json, os
path = '/etc/containers/policy.json'
p = json.load(open(path)) if os.path.exists(path) else {'default': [{'type': 'reject'}], 'transports': {}}
p.setdefault('transports', {}).setdefault('docker', {})[os.environ['IMAGE_REGISTRY']] = [
    {'type': 'sigstoreSigned', 'keyPath': '/etc/pki/containers/cosign.pub', 'signedIdentity': {'type': 'matchRepository'}}
]
json.dump(p, open(path, 'w'), indent=2)
PYEOF
fi
