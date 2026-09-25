//! ocioarchive -- archive a config and its LUT files or extract a config
//! archive (port of the OCIO `ocioarchive` app).

use ocio::config::archive::extract_ocioz_archive;
use ocio::{Config, OCIO_CONFIG_ARCHIVE_FILE_EXT};
use ocio_tools::argparse::ArgParse;
use std::io::Write;
use std::process::ExitCode;

fn archive(archive_name: &str, config_filename: &str) -> ExitCode {
    let config = if !config_filename.is_empty() {
        // Archive a config from a config file (e.g. /home/user/ocio/config.ocio).
        match Config::create_from_file(config_filename) {
            Ok(c) => c,
            Err(_) => {
                // Capture any errors and display a custom message.
                eprintln!("ERROR: Could not load config: {config_filename}");
                return ExitCode::from(1);
            }
        }
    } else if let Some(env) = ocio_tools::env_variable("OCIO").filter(|e| !e.is_empty()) {
        // Archive a config from the environment variable.
        println!("Archiving $OCIO={env}");
        match Config::create_from_env() {
            Ok(c) => c,
            Err(_) => {
                eprintln!("ERROR: Could not load config from $OCIO variable: {env}");
                return ExitCode::from(1);
            }
        }
    } else {
        eprintln!("ERROR: You must specify an input OCIO configuration.");
        return ExitCode::from(1);
    };

    // Do not add the ocioz extension if already present.
    let mut archive_name = archive_name.to_string();
    if !archive_name.ends_with(".ocioz") {
        archive_name.push_str(OCIO_CONFIG_ARCHIVE_FILE_EXT);
    }

    let mut file = match std::fs::File::create(&archive_name) {
        Ok(f) => f,
        Err(_) => {
            eprintln!(
                "Could not open output stream for: {archive_name}{OCIO_CONFIG_ARCHIVE_FILE_EXT}"
            );
            return ExitCode::from(1);
        }
    };

    match config.archive() {
        Ok(data) => {
            if let Err(e) = file.write_all(&data) {
                eprintln!("ERROR: {e}");
                return ExitCode::from(1);
            }
        }
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}

fn extract(archive_name: &str, destination: &str) -> ExitCode {
    let destination = if destination.is_empty() {
        // Set the default directory name to the name of the archive, without
        // the extension.
        ocio_tools::remove_extension(archive_name)
    } else {
        destination.to_string()
    };

    match extract_ocioz_archive(archive_name, &destination) {
        Ok(()) => {
            println!("{archive_name} has been extracted.");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn list(path: &str) -> ExitCode {
    let reader = std::fs::File::open(path)
        .ok()
        .and_then(|f| zip::ZipArchive::new(f).ok());
    let mut zip = match reader {
        Some(z) => z,
        None => {
            eprintln!("ERROR: File not found: {path}");
            return ExitCode::from(1);
        }
    };

    if zip.is_empty() {
        eprintln!("ERROR: Could not find the first entry in the archive.");
        return ExitCode::from(1);
    }

    println!("\nThe archive contains the following files:\n");
    println!("      Date     Time  CRC-32     Name");
    println!("      ----     ----  ------     ----");
    for i in 0..zip.len() {
        let entry = match zip.by_index_raw(i) {
            Ok(e) => e,
            Err(_) => {
                eprintln!("ERROR: Could not get information from entry: {i}");
                return ExitCode::from(1);
            }
        };
        let (month, day, year, hour, minute) = match entry.last_modified() {
            Some(d) => (d.month(), d.day(), d.year() % 100, d.hour(), d.minute()),
            None => (1, 1, 80, 0, 0),
        };
        println!(
            "      {:02}-{:02}-{:02} {:02}:{:02} {:08x}   {}",
            month,
            day,
            year,
            hour,
            minute,
            entry.crc32(),
            entry.name()
        );
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let argv = ocio_tools::command_line_args();
    let mut ap = ArgParse::new(
        "ocioarchive -- Archive a config and its LUT files or extract a config archive. \n\n\
         \x20   Note that any existing OCIOZ archive with the same name will be overwritten.\n\
         \x20   The .ocioz extension will be added to the archive name, if not provided.\n\n\
         Usage:\n\
         \x20   # Archive from the OCIO environment variable into myarchive.ocioz\n\
         \x20   ocioarchive myarchive\n\n\
         \x20   # Archive myconfig/config.ocio into myarchive.ocioz\n\
         \x20   ocioarchive myarchive --iconfig myconfig/config.ocio\n\n\
         \x20   # Extract myarchive.ocioz into new directory named myarchive\n\
         \x20   ocioarchive --extract myarchive.ocioz\n\n\
         \x20   # Extract myarchive.ocioz into new directory named ocio_config\n\
         \x20   ocioarchive --extract myarchive.ocioz --dir ocio_config\n\n\
         \x20   # List the files inside myarchive.ocioz\n\
         \x20   ocioarchive --list myarchive.ocioz\n",
    )
    .positional("")
    .separator("Options:")
    .option(
        "--iconfig %s",
        &["iconfig"],
        "Config to archive (takes precedence over $OCIO)",
    )
    .flag("--extract", "extract", "Extract an OCIOZ config archive")
    .option(
        "--dir %s",
        &["dir"],
        "Path where to extract the files (folders are created if missing)",
    )
    .flag(
        "--list",
        "list",
        "List the files inside an archive without extracting it",
    )
    .flag("--help", "help", "Display the help and exit")
    .flag("-h", "help", "Display the help and exit");

    if ap.parse(&argv).is_err() {
        eprintln!("{}", ap.geterror());
        return ExitCode::from(1);
    }

    let args = ap.args().to_vec();
    if ap.get_bool("help") || args.is_empty() {
        ap.print_usage();
        return ExitCode::SUCCESS;
    }

    let do_extract = ap.get_bool("extract");
    let do_list = ap.get_bool("list");

    if !do_extract && !do_list {
        // Archiving.
        if args.len() != 1 {
            eprintln!("ERROR: Missing the name of the archive to create.");
            return ExitCode::from(1);
        }
        archive(&args[0], &ap.get_string("iconfig", ""))
    } else if do_extract && !do_list {
        // Extracting.
        if args.len() != 1 {
            eprintln!("ERROR: Missing the name of the archive to extract.");
            return ExitCode::from(1);
        }
        extract(&args[0], &ap.get_string("dir", ""))
    } else if do_list && !do_extract {
        // Listing.
        list(&args[0])
    } else {
        eprintln!("Archive, extract, and/or list functions may not be used at the same time.");
        ExitCode::from(1)
    }
}
