## shell & data

````row
```sh
#!/usr/bin/env bash
set -euo pipefail
_deck="${1:-talk.md}"
if [ ! -f "${_deck}" ]; then
  echo "no ${_deck}" >&2
  exit 1
fi
deckhand "${_deck}"  # go
```
||
```json
{
  "title": "my talk",
  "columns": 4,
  "live": true,
  "notes": null,
  "theme": {
    "accent": "magenta"
  }
}
```
````

rust · python · js/ts · go · c/c++ · sh · json · yaml · toml · sql —
unknown languages stay plain
