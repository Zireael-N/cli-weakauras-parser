mod utils;

use clap::{App, AppSettings, Arg, ArgMatches, SubCommand};
use weakauras_parser as parser;

#[cfg(all(target_env = "musl", target_pointer_width = "64"))]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

fn try_main() -> Result<(), Error> {
    let matches = App::new("cli_weakauras_parser")
                          .settings(&[AppSettings::SubcommandRequiredElseHelp, AppSettings::VersionlessSubcommands])
                          .version("0.1.0")
                          .author("Velithris")
                          .about("Converts WA-compatible strings to JSON and vice versa")
                          .subcommand(SubCommand::with_name("decode")
                                      .about("Converts a WA-compatible string to JSON")
                                      .arg(Arg::with_name("INPUT FILE")
                                          .help("Sets the input file to use (- for stdin)")
                                          .required(true)
                                          .index(1))
                                      .arg(Arg::with_name("OUTPUT FILE")
                                            .help("Sets the output file to use")
                                            .long("output")
                                            .short("o")
                                            .takes_value(true)))
                          .subcommand(SubCommand::with_name("encode")
                                      .about("Converts a JSON string to a WA-compatible one")
                                      .arg(Arg::with_name("INPUT FILE")
                                          .help("Sets the input file to use (- for stdin)")
                                          .required(true)
                                          .index(1))
                                      .arg(Arg::with_name("OUTPUT FILE")
                                            .help("Sets the output file to use")
                                            .long("output")
                                            .short("o")
                                            .takes_value(true))
                                      .arg(Arg::with_name("VERSION")
                                            .help("Sets the version of a WA-compatible format (1 - the first version that uses FLATE compression, 2 - the first version that uses a binary serialization algorithm instead of AceSerializer)")
                                            .possible_values(&["1", "2"])
                                            .default_value("1")
                                            .long("wa_version")
                                            .short("-v")))
                          .get_matches();

    match matches.subcommand() {
        ("encode", Some(sub_m)) => encode(sub_m),
        ("decode", Some(sub_m)) => decode(sub_m),
        _ => unreachable!(),
    }
}

fn encode(matches: &ArgMatches<'_>) -> Result<(), Error> {
    let input_file = matches.value_of_os("INPUT FILE").unwrap();
    let output_file = matches.value_of_os("OUTPUT FILE");
    let json_string = utils::read_from_file(input_file)?;

    let wa_version = match matches.value_of("VERSION") {
        Some("1") => parser::StringVersion::Deflate,
        Some("2") => parser::StringVersion::BinarySerialization,
        _ => unreachable!(),
    };

    let lua_value = serde_json::from_str(&json_string)?;
    let wa_string = parser::encode(&lua_value, wa_version)?;

    utils::write_to_file(output_file, wa_string.as_bytes())?;

    Ok(())
}

fn decode(matches: &ArgMatches<'_>) -> Result<(), Error> {
    let input_file = matches.value_of_os("INPUT FILE").unwrap();
    let output_file = matches.value_of_os("OUTPUT FILE");
    let wa_string = utils::read_from_file(input_file)?;

    let lua_value = parser::decode(&wa_string)?;
    let json_string = serde_json::to_string_pretty(&lua_value)?;

    utils::write_to_file(output_file, json_string.as_bytes())?;

    Ok(())
}

fn main() {
    if let Err(err) = try_main() {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
}
