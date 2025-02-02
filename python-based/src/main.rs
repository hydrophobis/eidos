use serde::{Deserialize};
use std::{
    collections::{HashMap, HashSet},
    env,
    fs::File,
    io::{self, Write},
    process,
};

#[derive(Deserialize, Debug)]
struct LanguageConfig {
    statements: HashMap<String, StatementDef>,
    blocks: HashMap<String, BlockDef>,
    operators: HashMap<String, OperatorDef>,
}

#[derive(Deserialize, Debug)]
struct StatementDef {
    syntax: String,
    template: String,
}

#[derive(Deserialize, Debug)]
struct BlockDef {
    start: String,
    end: String,
    template: String,
}

#[derive(Deserialize, Debug)]
struct OperatorDef {
    symbol: String,
    template: String,
}

#[derive(Debug)]
enum Statement {
    Simple(String, Vec<String>),
    Block(String, Vec<Statement>),
}

fn load_config(file_path: &str) -> LanguageConfig {
    let file = File::open(file_path).unwrap_or_else(|_| {
        eprintln!("Could not open config file: {}", file_path);
        process::exit(1);
    });

    let config: LanguageConfig = serde_json::from_reader(file).unwrap_or_else(|err| {
        eprintln!("Error parsing config file: {}", err);
        process::exit(1);
    });

    config
}

fn parse_source(source: &str, config: &LanguageConfig) -> Vec<Statement> {
    let mut statements = Vec::new();
    let mut block_stack: Vec<(String, Vec<Statement>)> = Vec::new();

    for line in source.lines().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        let mut matched = false;

        for (block_name, block_def) in &config.blocks {
            if line.starts_with(&block_def.start) {
                block_stack.push((block_name.clone(), Vec::new()));
                matched = true;
                break;
            } else if !block_stack.is_empty() && line.starts_with(&block_def.end) {
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

        for (stmt_name, stmt_def) in &config.statements {
            if line.starts_with(&stmt_def.syntax) {
                let args_part = line[stmt_def.syntax.len()..].trim();
                let args: Vec<String> = args_part.split_whitespace().map(|s| s.to_string()).collect();

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

        if !matched {
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

fn generate_python_code(
    statements: &[Statement],
    config: &LanguageConfig,
    indent: usize,
) -> String {
    let indent_str = "    ".repeat(indent);
    let mut py_code = String::new();

    for stmt in statements {
        match stmt {
            Statement::Simple(name, args) => {
                if let Some(def) = config.statements.get(name) {
                    let mut line = def.template.clone();
                    for (i, arg) in args.iter().enumerate() {
                        let placeholder = format!("{{{}}}", i);
                        line = line.replace(&placeholder, arg);
                    }
                    py_code.push_str(&format!("{}{}
", indent_str, line));
                }
            }
            Statement::Block(name, inner) => {
                if let Some(def) = config.blocks.get(name) {
                    let inner_code = generate_python_code(inner, config, indent + 1);
                    let block_code = def.template.replace("{body}", &inner_code);
                    py_code.push_str(&format!("{}{}
", indent_str, block_code));
                }
            }
        }
    }

    py_code
}

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

    let config = load_config(config_file);
    if debug {
        println!("Language Config Loaded:\n{:#?}", config);
    }

    let source = std::fs::read_to_string(source_file).unwrap_or_else(|_| {
        eprintln!("Could not read source file: {}", source_file);
        process::exit(1);
    });

    let ast = parse_source(&source, &config);
    if debug {
        println!("AST: {:#?}", ast);
    }

    let mut py_code = String::new();
    py_code.push_str("def main():\n");
    py_code.push_str(&generate_python_code(&ast, &config, 1));
    py_code.push_str("\nif __name__ == \"__main__\":\n    main()\n");

    if let Err(e) = write_to_file(&py_code, "output.py") {
        eprintln!("Error writing output: {}", e);
        process::exit(1);
    }

    println!("Python code generated in output.py");
}