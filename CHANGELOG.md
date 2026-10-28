# Changelog

Each release's section is written from the typed subjects on `main`.

## 0.7.1 (2026-10-28)

### Fixes

- plan: leave a base's imports out of its witnesses (#57)

## 0.7.0 (2026-10-27)

### Features

- compose components and bases in one graph (#55)

### Dependencies

- deps: update dependency gitpython to v3.2.1 [security] (#49)

## 0.6.56 (2026-10-22)

### Fixes

- resolve: count exclusive providers per target (#46)

## 0.6.55 (2026-10-22)

### Features

- build: read no library at generate or build (#41)
- import: refuse an edited tracked module (#42)
- fetch: refresh tracked modules from CI (#43)

## 0.6.54 (2026-10-20)

### Fixes

- import: list a held module and skip nested ones (#39)

## 0.6.53 (2026-10-20)

### Fixes

- emit: relocate /usr/local like /opt (#37)

## 0.6.52 (2026-10-19)

### Features

- add the threat model and close trust gaps (#33)
- import: copy modules and record them in a lock (#34)

### Fixes

- refuse the ext transport for asset pins (#30)
- lint audit (#31)
- order nothing for an after with no provider (#35)

### Dependencies

- deps: bump common (#26)
- deps: bump common (#32)

## 0.6.51 (2026-10-10)

### Features

- simplify repository editing (#16)
- schema: file versions, typed sources, declared libraries (#21)
- import: group modules and map providers (#22)

### Fixes

- create: prompt for module declarations (#14)

## 0.6.50 (2026-10-02)

### Features

- add a security-policy network rule (#7)
- nested modules, generated scripts and a docs site (#8)

### Fixes

- bump common, pin choices read optional (#9)

## 0.6.49 (2026-09-27)

### Features

- cli: parse with clap and generate the command and schema docs
- build installer media without fisherman
- release: ship an SBOM with each release (#4)

## 0.6.48 and earlier

The history before 0.6.49 predates the typed pull request titles. The notes on
each GitHub release record what it changed.
