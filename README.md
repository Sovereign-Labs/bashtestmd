# Bashtestmd

![crates.io](https://img.shields.io/crates/v/bashtestmd)

Compiles Markdown files with Bash code blocks into a single Bash script for CI testing purposes.

## Basic Usage

The following command parses all code blocks from the input markdown file and generates a script
containing all blocks tagged with the specified `TAG_NAME`

```sh
bashtestmd --input {PATH_TO_INPUT_FILE} --output {OUTPUT_FILE_NAME} --tag {TAG_NAME}`.
```

For example, `bashtestmd --input README.md --output demo-readme.sh --tag test-ci` will find all code blocks
of the form and generate a script which runs them sequentially, enforcing that each command exits with status code `0`.

````
```sh,test-ci
$ echo "This is a demo"
```
````

Note that `bashtestmd` only interprets lines beginning with `$` as commands. This allows output to be included in
snippets without compromising the generated script.

````
```sh,test-ci
$ echo "This is a demo"
This is a demo
```
````

## Supported tags

`bashtestmd` supports the following optional tags on code blocks:

1. `bashtestmd:compare-output`
1. `bashtestmd:exit-code-ignore`
1. `bashtestmd:exit-code="{EXPECTED_CODE}"`
1. `bashtestmd:long-running`
1. `bashtestmd:wait-until="{TEXT}"`

### Compare Output

The tag `bashtestmd:compare-output` causes the generated script to check that the command output
matches the output in the markdown file.

For example, this command will fail if the server returns `"goodbye"` in response to a query to `localhost:80/hello`
instead of the expected `"hello, world"`

````
```sh,test-ci,bashtestmd:compare-output`
$ curl localhost:80/hello
"hello, world"
```
````

### Exit Code Ignore

The tag `bashtestmd:exit-code-ignore` causes `bashtestmd` to ignore the exit code of the command rather than enforcing that the code is `0`

### Exit Code

The tag `bashtestmd:exit-code="{CODE}"` causes `bashtestmd` to check that the exit code of the command matches the provided value

````
```sh,test-ci,bashtestmd:exit-code="1"`
$ rm this_file_does_not_exist.txt
rm: this_file_does_not_exist.txt: No such file or directory
```
````

### Long Running

The tag `bashtestmd:long-running` causes the command to run in the background and waits 120 seconds for the task to complete.
It is strongly recommended to combine `long-running` with `wait-until` in order to override this default behavior.

For example, the following commmand compiles to `cargo run &; sleep 120`

````
```sh,test-ci,bashtestmd:long-running`
$ cargo run
```
````

### Wait Until

The tag `bashtestmd:wait-until={SOME_TEXT}` will cause the script to wait for the process to output the expected text
before continuing rather than simply sleeping for two minutes. Note that this command **_requires_** the `long-running` tag in order to have an effect.

````
```sh,test-ci,bashtestmd:long-running,bashtestmd:wait-until="Finished release"`
$ cargo build --release
```
````

## Local Installation

To set up `bashtestmd` for local development

1. `git clone https://github.com/Sovereign-Labs/bashtestmd.git`
1. `cargo install --path bashtestmd/`
