2 modules, the ungated set.

| Module | Description | Options | Satisfies |
| --- | --- | --- | --- |
| `kernel/signed-kernel` | A kernel that signs its own modules against a Secure Boot key |  | `cis-fedora: 1.1.1.1, 5.2.20` `stig: RHEL-09-232010` |
| `drivers/kvmfr` | A DKMS module built and signed against the kernel above |  |  |
