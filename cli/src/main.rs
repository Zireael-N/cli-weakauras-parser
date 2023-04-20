mod utils;

use clap::{builder::PossibleValuesParser, Arg, ArgMatches, Command};
use std::ffi::OsString;
use weakauras_parser as parser;

#[cfg(all(target_env = "musl", target_pointer_width = "64"))]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

fn try_main() -> Result<(), Error> {
    let matches = Command::new("cli_weakauras_parser")
        .version("0.1.2")
        .author("Velithris")
        .about("Converts WA-compatible strings to JSON and vice versa")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(Command::new("decode")
            .about("Converts a WA-compatible string to JSON")
            .disable_version_flag(true)
            .arg_required_else_help(true)
            .arg(Arg::new("INPUT FILE")
                .help("Sets the input file to use (- for stdin)")
                .value_parser(clap::value_parser!(OsString))
                .required(true)
                .index(1))
            .arg(Arg::new("OUTPUT FILE")
                .help("Sets the output file to use")
                .long("output")
                .short('o')
                .value_parser(clap::value_parser!(OsString))))
        .subcommand(Command::new("encode")
            .about("Converts a JSON string to a WA-compatible one")
            .disable_version_flag(true)
            .arg_required_else_help(true)
            .arg(Arg::new("INPUT FILE")
                .help("Sets the input file to use (- for stdin)")
                .value_parser(clap::value_parser!(OsString))
                .required(true)
                .index(1))
            .arg(Arg::new("OUTPUT FILE")
                .help("Sets the output file to use")
                .long("output")
                .short('o')
                .value_parser(clap::value_parser!(OsString)))
            .arg(Arg::new("VERSION")
                .help("Sets the version of a WA-compatible format (1 - the first version that uses FLATE compression, 2 - the first version that uses a binary serialization algorithm instead of AceSerializer)")
                .value_parser(PossibleValuesParser::new(["1", "2"]))
                .default_value("1")
                .long("wa_version")
                .short('v')))
        .get_matches();

    match matches.subcommand() {
        Some(("encode", sub_m)) => encode(sub_m),
        Some(("decode", sub_m)) => decode(sub_m),
        _ => unreachable!(),
    }
}

fn encode(matches: &ArgMatches) -> Result<(), Error> {
    let input_file = matches.get_one::<OsString>("INPUT FILE").unwrap();
    let output_file = matches.get_one::<OsString>("OUTPUT FILE");
    let json_string = utils::read_from_file(input_file)?;

    let wa_version = match matches.get_one::<String>("VERSION").map(|s| s.as_str()) {
        Some("1") => parser::StringVersion::Deflate,
        Some("2") => parser::StringVersion::BinarySerialization,
        _ => unreachable!(),
    };

    let lua_value = serde_json::from_str(&json_string)?;
    let wa_string = parser::encode(&lua_value, wa_version)?;

    utils::write_to_file(output_file.map(|s| s.as_os_str()), wa_string.as_bytes())?;

    Ok(())
}

fn decode(matches: &ArgMatches) -> Result<(), Error> {
    let input_file = matches.get_one::<OsString>("INPUT FILE").unwrap();
    let output_file = matches.get_one::<OsString>("OUTPUT FILE");
    let wa_string = utils::read_from_file(input_file)?;

    let lua_value = parser::decode(&wa_string)?;
    let json_string = serde_json::to_string_pretty(&lua_value)?;

    utils::write_to_file(output_file.map(|s| s.as_os_str()), json_string.as_bytes())?;

    Ok(())
}

fn main() {
    if let Err(err) = try_main() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}
