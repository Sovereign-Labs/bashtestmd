use std::collections::VecDeque;
use std::io::{self, Write};

use clap::Parser;
use indoc::indoc;
use markdown::mdast;

#[derive(Debug, Parser)]
struct Args {
    /// Input Markdown file to parse
    #[clap(short, long)]
    input: String,
    /// Path to output Bash script
    #[clap(short, long)]
    output: String,
    /// Only run code blocks with this tag
    #[clap(short, long)]
    tag: String,
}

fn main() {
    let args = Args::parse();

    let file_contents = std::fs::read_to_string(&args.input).unwrap();
    let markdown_parse_options = markdown::ParseOptions::gfm();
    let markdown_ast = markdown::to_mdast(&file_contents, &markdown_parse_options).unwrap();

    let code_blocks = get_all_code_blocks(markdown_ast);
    let commands = convert_code_blocks_into_commands(code_blocks, &args.tag);
    let script = compile_commands_into_bash(commands);

    std::fs::write(&args.output, script).unwrap();
}

struct Command {
    cmd: String,
    long_running: bool,
    expected_output: Option<String>,
    wait_until: Option<String>,
    exit_code: Option<i32>,
}

impl Command {
    fn new(cmd: &str) -> Self {
        Self {
            cmd: cmd.to_string(),
            long_running: false,
            expected_output: None,
            wait_until: None,
            exit_code: Some(0),
        }
    }

    fn compile(&self, mut w: impl io::Write) -> io::Result<()> {
        writeln!(
            w,
            "echo {}",
            shell_escape::escape(format!("Running: '{}'", self.cmd).into())
        )?;

        if let Some(exit_code) = self.exit_code {
            // expected_output does recording of proper exit code:
            let exit_code_grabber = if self.expected_output.is_none() {
                "exit_code=$?\n"
            } else {
                "\n"
            };
            writeln!(
                w,
                indoc!(
                    r#"
                    {1}
                    if [ $exit_code -ne {0} ]; then
                        echo "Expected exit code {0}, got $exit_code"
                        check_and_output_long_running_output
                        exit 1
                    fi
                    "#,
                ),
                exit_code, exit_code_grabber,
            )?;
        }

        if self.long_running {
            if let Some(wait_until) = &self.wait_until {
                writeln!(
                    w,
                    indoc!(
                        r#"
                        output=$(mktemp)
                        export BASHTESTMD_LONG_RUNNING_OUTPUT=$output
                        {} &> $output &
                        background_process_pid=$!
                        echo "Waiting for process with PID: $background_process_pid to have a match in $output"
                        until grep -q -i {} $output
                        do
                          if ! ps $background_process_pid > /dev/null
                          then
                            echo "The background process died, output:" >&2
                            cat $output
                            exit 1
                          fi
                          echo -n "."
                          sleep 5
                        done
                        echo ""
                        "#
                    ),
                    self.cmd,
                    shell_escape::escape(wait_until.into())
                )?;
            } else {
                // No expected output, just run the command and wait two
                // minutes. Very, very hackish.
                writeln!(w, "{} &", self.cmd)?;
                writeln!(w, "sleep 120")?;
            }
            return Ok(());
        }

        if let Some(output) = &self.expected_output {
            writeln!(
                w,
                indoc!(
                    r#"
                    output=$({})
                    exit_code=$?
                    expected={}
                    # Either of the two must be a substring of the other. This kinda protects us
                    # against whitespace differences, trimming, etc.
                    if ! [[ $output == *"$expected"* || $expected == *"$output"* ]]; then
                        echo "'$expected' not found in text:"
                        echo "'$output'"
                        check_and_output_long_running_output
                        echo "=========== END OF THE LONG RUNNING OUTPUT. Terminating..."
                        exit 1
                    fi
                    "#
                ),
                self.cmd,
                shell_escape::escape(output.into())
            )?;
        } else {
            writeln!(w, "{}", self.cmd)?;
        }

        Ok(())
    }
}

fn compile_commands_into_bash(cmds: Vec<Command>) -> String {
    let mut script = Vec::<u8>::new();
    // Shebang.
    writeln!(&mut script, "#!/usr/bin/env bash").unwrap();
    // allow aliases in scripts. ideally we would execute bash in interactive mode (`-i`)
    // to make the script run closer to how user runs commands from readme, but flags in
    // shebang aren't cross platfrom
    writeln!(&mut script, "shopt -sq expand_aliases").unwrap();
    writeln!(&mut script, r#"trap 'jobs -p | xargs -r kill' EXIT"#).unwrap();
    writeln!(
        &mut script,
        indoc!(
        r#"
        check_and_output_long_running_output() {{
            if [[ -n "$BASHTESTMD_LONG_RUNNING_OUTPUT" && -f "$BASHTESTMD_LONG_RUNNING_OUTPUT" ]]; then
                echo "Output of the long running task:"
                cat "$BASHTESTMD_LONG_RUNNING_OUTPUT"
            fi
        }}
        "#
        )
    ).unwrap();

    for cmd in cmds {
        cmd.compile(&mut script).unwrap();
    }
    writeln!(&mut script, r#"echo "All tests passed!"; exit 0"#).unwrap();
    String::from_utf8(script).unwrap()
}

struct CodeBlockTags {
    long_running: bool,
    compare_output: bool,
    exit_code: Option<i32>,
    wait_until: Option<String>,
    raw: bool,
}

impl CodeBlockTags {
    fn parse(code_block: &mdast::Code, only_tag: &str) -> Self {
        let langs: Vec<String> = code_block
            .lang
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .map(str::to_string)
            .collect();

        let mut tags = Self {
            long_running: false,
            compare_output: false,
            exit_code: Some(0),
            wait_until: None,
            raw: false,
        };

        for (idx, lang) in langs.into_iter().enumerate() {
            if lang == "bashtestmd:long-running" {
                tags.long_running = true;
            } else if lang == "bashtestmd:compare-output" {
                tags.compare_output = true;
            } else if lang == "bashtestmd:exit-code-ignore" {
                tags.exit_code = None;
            } else if lang == "bashtestmd:raw" {
                tags.raw = true;
            } else if lang.starts_with("bashtestmd:exit-code=") {
                let exit_code = lang.split_once('=').unwrap().1.parse().unwrap();
                tags.exit_code = Some(exit_code);
            } else if lang.starts_with("bashtestmd:wait-until=") {
                let wait_until = lang.split_once('=').unwrap().1.to_string();
                tags.wait_until = Some(wait_until);
            } else {
                // Don't warn on the first `lang` tag of if the tag is the one marking blocks for bashtestmd to compile
                // This ensures that (i.e. ```rust,test-ci```) should not generate warnings.
                if idx != 0 && lang != only_tag {
                    println!("Unknown bashtestmd tag, ignoring: {lang}");
                }
            }
        }

        if tags.raw && tags.compare_output {
            eprintln!(
                "Tags `bashtestmd:raw` and `bashtestmd:compare-output` are mutually exclusive"
            );
            std::process::exit(1);
        }

        tags
    }
}

fn convert_code_blocks_into_commands(
    code_blocks: Vec<mdast::Code>,
    only_tag: &str,
) -> Vec<Command> {
    const PROMPT: &str = "$ ";

    let mut commands = Vec::new();

    for code_block in code_blocks {
        if !code_block
            .lang
            .as_deref()
            .unwrap_or_default()
            .contains(only_tag)
        {
            continue;
        }
        let mut block_contains_command = false;
        let tags = CodeBlockTags::parse(&code_block, only_tag);

        let mut cmd: Option<String> = None;
        let mut output = String::new();

        for line in code_block.value.lines() {
            if let Some(cmd_string) = line.strip_prefix(PROMPT) {
                if let Some(cmd) = cmd {
                    commands.push(Command::new(&cmd));
                }
                cmd = Some(cmd_string.to_string());
                block_contains_command = true;
            } else if tags.raw {
                if let Some(cmd) = cmd.as_mut() {
                    cmd.push('\n');
                    cmd.push_str(line);
                }
            } else {
                output.push_str(line);
                output.push('\n');
            }
        }
        if !block_contains_command {
            println!(
                "Warning: could not find command in block`:\n```\n{}\n```",
                &code_block.value
            );
            println!("^^^^^ remove the tag {only_tag} from the block or add a command beginning with `{PROMPT}` to fix this warning");
        }
        if let Some(cmd) = cmd {
            let mut cmd = Command::new(&cmd);
            cmd.long_running = tags.long_running;
            cmd.wait_until = tags.wait_until;
            cmd.expected_output = if tags.compare_output {
                Some(output)
            } else {
                None
            };
            commands.push(cmd);
        }
    }

    commands
}

/// Ordered list of all code blocks in the Markdown file.
fn get_all_code_blocks(markdown_ast: mdast::Node) -> Vec<mdast::Code> {
    let mut code_blocks = Vec::new();

    let mut nodes: VecDeque<mdast::Node> = markdown_ast
        .children()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();

    while let Some(next_node) = nodes.pop_front() {
        if let mdast::Node::Code(code_node) = next_node {
            code_blocks.push(code_node);
        } else {
            let children = next_node.children().map(Vec::as_slice).unwrap_or_default();
            for child in children.iter() {
                nodes.push_front(child.clone());
            }
        }
    }

    code_blocks
}
