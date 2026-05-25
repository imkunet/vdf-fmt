# vdf-fmt

opinionated [KeyValues](https://developer.valvesoftware.com/wiki/KeyValues) "VDF" formatter.

## configuration

there is no configuration (except for the TWO flags). you will accept my opinions without questions.

## usage

```sh
# to stdout
vdf-fmt path/to/file.vdf

# write back to file
vdf-fmt -w path/to/file.vdf

# multiple files are written in place
vdf-fmt file-a.vdf file-b.vdf

# also reflow comments that look like disabled key-value pairs
vdf-fmt --reflow-comments path/to/file.vdf

# leave booleans and numbers unquoted
vdf-fmt --bare-literals path/to/file.vdf
```

