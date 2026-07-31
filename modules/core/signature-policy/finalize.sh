python3 << 'PYEOF'
import json, os
path = '/etc/containers/policy.json'
p = json.load(open(path)) if os.path.exists(path) else {'default': [{'type': 'reject'}], 'transports': {}}
p.setdefault('transports', {}).setdefault('docker', {})['ghcr.io/tectonic-os'] = [
    {'type': 'sigstoreSigned', 'keyPath': '/etc/pki/containers/cosign.pub', 'signedIdentity': {'type': 'matchRepository'}}
]
json.dump(p, open(path, 'w'), indent=2)
PYEOF
