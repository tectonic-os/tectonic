2 modules, the ungated set.

| Module | Description | Options | Satisfies | Refuses |
| --- | --- | --- | --- | --- |
| `one/contradicts` | Claims a number and refuses the rule that number reaches |  | `cis-fedora: 1.1.1.1` | `package_aide_installed` |
| `one/typo` | Refuses a rule the content carries nothing by |  | `stig: CCI-000198` | `not_a_rule_this_content_has` `sshd_disable_root_login` (lifted) |
