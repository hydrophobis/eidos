use serde::{Deserialize};
use std::{
    collections::{HashMap, HashSet},
    env,
    fs::File,
    io::{self, Write},
    process,
};

/// The overall config for the language definition. Notice that every aspect of the language is defined in JSON.
#[derive(Deserialize, Debug)]
struct LanguageConfig {
    /// Define the different types of statements. The key is an identifier (like "print" or "assignment")
    statements: HashMap<String, StatementDef>,
    /// Blocks (or scopes) can be defined with custom start/end tokens and even custom evaluation templates.
    blocks: HashMap<String, BlockDef>,
    /// Operators (and their evaluation templates) so that arithmetic or other operations can be fully configured.
    operators: HashMap<String, OperatorDef>,
}

/// A statement definition in the language. Here, 'syntax' is a pattern or prefix that identifies the statement,
/// and 'template' is a string that is used for code generation (for instance, C code).
#[derive(Deserialize, Debug)]
struct StatementDef {
    syntax: String,   // e.g. "print", "let", etc.
    template: String, // e.g. "printf(\"%d\\n\", {0});", or for assignment "int {0} = {1};"
}

/// A block definition. Blocks are more advanced; they have a start token, an end token, and a template for code generation.
#[derive(Deserialize, Debug)]
struct BlockDef {
    start: String,    // E.g. "if (" or "while ("
    end: String,      // E.g. "end if" or a closing brace token.
    template: String, // E.g. "if ({condition}) {{\n{body}\n}}" — you can define placeholders.
}

/// Operator definitions let you define operations (like +, -, etc.) via a template.
#[derive(Deserialize, Debug)]
struct OperatorDef {
    symbol: String,   // E.g. "+"
    template: String, // E.g. "({0} + {1})"
}

/// We define a very simple AST that can hold different types of statements.
#[derive(Debug)]
enum Statement {
    /// A simple statement with potential arguments (captured as strings)
    Simple(String, Vec<String>),
    /// A block statement: its name (matching a block defined in JSON) and its inner statements.
    Block(String, Vec<Statement>),
}

/// Load the language configuration from a JSON file.
fn load_config(file_path: &str) -> LanguageConfig {
    let file = File::open(file_path).unwrap_or_else(|_| {
        eprintln!("Could not open config file: {}", file_path);
        process::exit(1);
    });

    let config: LanguageConfig = serde_json::from_reader(file).unwrap_or_else(|err| {
        eprintln!("Error parsing config file: {}", err);
        process::exit(1);
    });

    // No longer printing config here.
    config
}

/// A very simple parser that uses the JSON definitions to build an AST.
/// It splits the source code into lines and then:
///   - Checks if the line matches any statement syntax
///   - Checks for block start/end tokens to build nested stuff
fn parse_source(source: &str, config: &LanguageConfig) -> Vec<Statement> {
    let mut statements = Vec::new();
    let mut block_stack: Vec<(String, Vec<Statement>)> = Vec::new(); // (block name, statements inside)

    // Iterate through each line of the source code.
    for line in source.lines().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        let mut matched = false;

        // Check if we are starting a new block.
        for (block_name, block_def) in &config.blocks {
            if line.starts_with(&block_def.start) {
                // Push a new block on the stack.
                block_stack.push((block_name.clone(), Vec::new()));
                matched = true;
                break;
            } else if !block_stack.is_empty() && line.starts_with(&block_def.end) {
                // End of the current block.
                if let Some((bname, inner_statements)) = block_stack.pop() {
                    let block_stmt = Statement::Block(bname, inner_statements);
                    if let Some((_, outer)) = block_stack.last_mut() {
                        outer.push(block_stmt);
                    } else {
                        statements.push(block_stmt);
                    }
                    matched = true;
                    break;
                }
            }
        }
        if matched { continue; }

        // Check for simple statements
        for (stmt_name, stmt_def) in &config.statements {
            if line.starts_with(&stmt_def.syntax) {
                // Extract arguments after the syntax.
                // Here we assume arguments are space separated after the syntax.
                let args_part = line[stmt_def.syntax.len()..].trim();
                let args: Vec<String> = args_part
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();

                let simple_stmt = Statement::Simple(stmt_name.clone(), args);
                if let Some((_, block)) = block_stack.last_mut() {
                    block.push(simple_stmt);
                } else {
                    statements.push(simple_stmt);
                }
                matched = true;
                break;
            }
        }

        // If nothing matched, treat it as an expression or default print
        if !matched {
            println!("No matches found on line: {}", line);
            let simple_stmt = Statement::Simple("print".to_string(), vec![line.to_string()]);
            if let Some((_, block)) = block_stack.last_mut() {
                block.push(simple_stmt);
            } else {
                statements.push(simple_stmt);
            }
        }
    }

    statements
}

/// Generate C code from the AST using the JSON templates for each statement or block.
/// This is a basic implementation: you could extend it to do full template replacement
/// (e.g. using regex or a templating engine) for more dynamic code generation.
fn generate_c_code(
    statements: &[Statement],
    config: &LanguageConfig,
    declared_vars: &mut HashSet<String>,
    indent: usize,
) -> String {
    let indent_str = "    ".repeat(indent);
    let mut c_code = String::new();

    for stmt in statements {
        match stmt {
            Statement::Simple(name, args) => {
                // Look up the statement definition by name.
                if let Some(def) = config.statements.get(name) {
                    // Do a simple replacement: {0}, {1}, etc.
                    let mut line = def.template.clone();
                    for (i, arg) in args.iter().enumerate() {
                        let placeholder = format!("{{{}}}", i);
                        line = line.replace(&placeholder, arg);
                    }
                    // For assignments, ensure variables are declared only once.
                    if name == "assignment" && !args.is_empty() {
                        let var_name = &args[0];
                        if !declared_vars.contains(var_name) {
                            c_code.push_str(&format!("{}int {};\n", indent_str, var_name));
                            declared_vars.insert(var_name.clone());
                        }
                    }
                    c_code.push_str(&format!("{}{}\n", indent_str, line));
                }
            }
            Statement::Block(name, inner) => {
                if let Some(def) = config.blocks.get(name) {
                    // Use the block template.
                    // We assume the template has placeholders like {body} that we fill in recursively.
                    let inner_code = generate_c_code(inner, config, declared_vars, indent + 1);
                    let block_code = def.template.replace("{body}", &inner_code);
                    c_code.push_str(&format!("{}{}\n", indent_str, block_code));
                }
            }
        }
    }

    c_code
}

/// Write generated code to a file.
fn write_to_file(code: &str, file_path: &str) -> io::Result<()> {
    let mut file = File::create(file_path)?;
    file.write_all(code.as_bytes())?;
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut debug = false;
    let (config_file, source_file) = match args.len() {
        4 if args[1] == "-d" => {
            debug = true;
            (&args[2], &args[3])
        },
        3 => (&args[1], &args[2]),
        _ => {
            eprintln!("Usage: eidos [-d] <config_file> <source_file>");
            process::exit(1);
        }
    };

    // Load our language definition from JSON.
    let config = load_config(config_file);
    if debug {
        println!("Language Config Loaded:\n{:#?}", config);
    }

    // Read the source code.
    let source = std::fs::read_to_string(source_file).unwrap_or_else(|_| {
        eprintln!("Could not read source file: {}", source_file);
        process::exit(1);
    });

    // Parse the source into an AST using our JSON definitions.
    let ast = parse_source(&source, &config);
    if debug {
        println!("AST: {:#?}", ast);
    }

    // Generate C code. (You could later swap this out to generate another target language!)
    let mut declared_vars = HashSet::new();
    let mut c_code = String::new();
    c_code.push_str("#include <stdio.h>\n\n");
    c_code.push_str("int main() {\n");
    c_code.push_str(&generate_c_code(&ast, &config, &mut declared_vars, 1));
    c_code.push_str("    return 0;\n");
    c_code.push_str("}\n");

    // Write the generated code to output.c.
    if let Err(e) = write_to_file(&c_code, "output.c") {
        eprintln!("Error writing output: {}", e);
        process::exit(1);
    }

    println!("C code generated in output.c");
}
