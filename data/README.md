# data/ (private, never committed)

Put the files `dc` writes about your own computer here: scans, app snapshots, view files.
Everything in this folder except this README is ignored by git, because these files list what is
on your disk.

```powershell
cargo run -q -p dc-cli -- apps --out data/apps.json
cargo run -q -p dc-cli -- scan "C:\Users\<you>" --out data/me.json
cargo run -q -p dc-cli -- scan "C:\ProgramData" --out data/scan-pd.json
cargo run -q -p dc-cli -- leftovers data/me.json data/scan-pd.json --apps data/apps.json
cargo run -q -p dc-cli -- view-data data/me.json data/scan-pd.json --apps data/apps.json --out data/view.json
```
