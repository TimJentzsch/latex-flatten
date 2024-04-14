use clap::Parser;
use regex::{Captures, Regex};
use std::{
    borrow::Cow,
    ffi::OsString,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::exit,
};
use thiserror::Error;
use walkdir::{DirEntry, WalkDir};

/// The supported types of input and output
#[derive(Debug, PartialEq, Eq)]
enum FileType {
    Directory,
    Zip,
}

#[derive(Debug, Error)]
enum FileTypeError {
    #[error("Invalid extension .{0:?}, expected zip file or directory")]
    InvalidExtension(OsString),
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The path of the folder containing the LaTeX project
    #[arg(short, long)]
    path: Box<Path>,

    /// The path of the directory where the new project will be created
    #[arg(short, long)]
    out: Box<Path>,
}

fn main() {
    let args = Args::parse();

    let input_path = &args.path;
    let input_type = path_file_type(input_path).unwrap_or_else(|err| {
        eprintln!("Input: {err}");
        exit(1);
    });

    // Sanity checks
    match input_type {
        FileType::Directory => {
            if !input_path.is_dir() {
                eprintln!("The input path must point to a directory");
                exit(1);
            }
        }
        FileType::Zip => {
            if !input_path.is_file() {
                eprintln!("The input path must point to a zip file");
                exit(1);
            }
        }
    }

    let output_path = &args.out;
    let output_type = path_file_type(output_path).unwrap_or_else(|err| {
        eprintln!("Output: {err}");
        exit(1);
    });

    match output_type {
        FileType::Directory => {
            if output_path.exists() {
                if output_path.is_file() {
                    eprintln!("Expected the output path to be an empty directory");
                    exit(1);
                } else if output_path.read_dir().unwrap().next().is_some() {
                    eprintln!("The output directory must be empty");
                    exit(1);
                }
            } else {
                fs::create_dir_all(output_path).unwrap();
            }
        }
        FileType::Zip => {
            if !input_path.is_file() {
                println!("The input path must point to a zip file");
                exit(1);
            }
        }
    }

    if input_type == FileType::Directory && output_type == FileType::Directory {
        // Traverse folder structure
        WalkDir::new(input_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .for_each(|e| process_entry(e, &args));
    } else {
        todo!("Support zip files")
    }
}

fn path_file_type(path: &Path) -> Result<FileType, FileTypeError> {
    if let Some(extension) = path.extension() {
        if extension.to_ascii_lowercase() == "zip" {
            Ok(FileType::Zip)
        } else {
            Err(FileTypeError::InvalidExtension(extension.to_owned()))
        }
    } else {
        Ok(FileType::Directory)
    }
}

fn process_entry(entry: DirEntry, args: &Args) {
    let new_path = args.out.to_owned().join(flatten_path(entry.path(), args));

    let new_content = process_content(&entry);

    let mut new_file = File::create(&new_path)
        .unwrap_or_else(|_| panic!("Failed to create new file {new_path:?}"));
    new_file
        .write_all(&new_content)
        .expect("Failed to write to file");
}

fn flatten_path(path: &Path, args: &Args) -> PathBuf {
    let root_components = args.path.components().count();
    let components: Vec<_> = path
        .components()
        .skip(root_components)
        .map(|component| component.as_os_str().to_str().unwrap().to_string())
        .collect();
    components.join("__").into()
}

fn process_content(entry: &DirEntry) -> Vec<u8> {
    let mut file = File::open(entry.path()).expect("Failed to open file");

    if !entry.path().extension().map_or(false, |ext| ext == "tex") {
        // For non-tex files, just return the content
        let mut content = Vec::new();
        file.read_to_end(&mut content)
            .expect("Failed to read file content");
        return content;
    }

    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();

    let new_lines: Vec<_> = content.lines().map(replace_imports).collect();

    new_lines.join("\n").into_bytes()
}

fn replace_imports(line: &str) -> Cow<'_, str> {
    let reg =
        Regex::new(r"\\(input|include|includegraphics|bibliography\w*)(\[[^]]*\])?\{([^}]*)\}")
            .unwrap();

    reg.replace_all(line, |capture: &Captures| {
        format!(
            "\\{}{}{{{}}}",
            // Command type
            capture.get(1).unwrap().as_str(),
            // Options
            capture.get(2).map(|mat| mat.as_str()).unwrap_or(""),
            // Flatten the paths
            capture.get(3).unwrap().as_str().replace('/', "__")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_imports_input() {
        let line = r"\input{content/background}";
        let expected = r"\input{content__background}";

        assert_eq!(replace_imports(line), expected);
    }

    #[test]
    fn test_replace_imports_include() {
        let line = r"\include{content/background}";
        let expected = r"\include{content__background}";

        assert_eq!(replace_imports(line), expected);
    }

    #[test]
    fn test_replace_imports_bibliography() {
        let line = r"\bibliography{bibliography/references}";
        let expected = r"\bibliography{bibliography__references}";

        assert_eq!(replace_imports(line), expected);
    }

    #[test]
    fn test_replace_imports_bibliography_custom() {
        let line = r"\bibliographyS{bibliography/references}";
        let expected = r"\bibliographyS{bibliography__references}";

        assert_eq!(replace_imports(line), expected);
    }

    #[test]
    fn test_replace_imports_includegraphics() {
        let line = r"\includegraphics{figures/search_process.pdf}";
        let expected = r"\includegraphics{figures__search_process.pdf}";

        assert_eq!(replace_imports(line), expected);
    }

    #[test]
    fn test_replace_imports_includegraphics_options() {
        let line = r"\includegraphics[width=0.8\linewidth]{figures/search_process.pdf}";
        let expected = r"\includegraphics[width=0.8\linewidth]{figures__search_process.pdf}";

        assert_eq!(replace_imports(line), expected);
    }
}
