use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

#[derive(Debug, PartialEq, Eq)]
struct InitializedWorkspace {
    path: PathBuf,
    created: bool,
}

pub(crate) fn run<I>(args: I) -> ExitCode
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    if matches!(args.as_slice(), [argument] if matches!(argument.as_str(), "help" | "--help" | "-h"))
    {
        print_help();
        return ExitCode::SUCCESS;
    }
    let directory = match parse_args(&args) {
        Ok(directory) => directory,
        Err(error) => {
            eprintln!("ERRORE init: {error}");
            return ExitCode::from(1);
        }
    };
    match initialize(&directory) {
        Ok(initialized) => {
            let action = if initialized.created {
                "creato"
            } else {
                "inizializzato"
            };
            println!("Workspace {action}: {}", initialized.path.display());
            println!(
                "Prossimo passo: cd {} && antiskill ingest DIR",
                initialized.path.display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ERRORE init: {error}");
            ExitCode::from(1)
        }
    }
}

fn parse_args(args: &[String]) -> Result<PathBuf, String> {
    match args {
        [] => Err("Manca DIR. Uso: antiskill init DIR".into()),
        [directory] if !directory.starts_with('-') => Ok(PathBuf::from(directory)),
        [argument] => Err(format!(
            "Argomento inatteso: {argument}. Uso: antiskill init DIR"
        )),
        _ => Err("È ammesso esattamente un argomento DIR. Uso: antiskill init DIR".into()),
    }
}

fn print_help() {
    println!(
        "\
Crea un nuovo workspace Antiskill.

USO:
    antiskill init DIR

DIR può essere un percorso relativo o assoluto. Il parent deve esistere e non
può attraversare symlink. È accettata anche una directory esistente, purché
reale e completamente vuota. I dati del workspace verranno creati soltanto dal
successivo `antiskill ingest DIR`.
"
    );
}

fn validate_shape(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path.file_name().is_none()
        || path.components().any(|component| {
            !matches!(
                component,
                Component::Prefix(_)
                    | Component::RootDir
                    | Component::CurDir
                    | Component::Normal(_)
            )
        })
    {
        return Err(format!("Percorso workspace non sicuro: {}", path.display()));
    }
    Ok(())
}

fn real_canonical_parent(parent: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(parent).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "Parent del workspace inesistente".to_owned()
        } else {
            format!("Parent del workspace non verificabile: {error}")
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Parent del workspace non è una directory reale".into());
    }

    // Do not use `canonicalize(parent) == parent` as a symlink test. The same
    // real Windows path may legitimately be spelled as an 8.3 alias or as an
    // extended `\\?\` path after canonicalization. Instead, inspect every
    // existing ancestor without following the ancestor itself and reject actual
    // symlink components explicitly.
    for ancestor in parent
        .ancestors()
        .filter(|ancestor| !ancestor.as_os_str().is_empty())
    {
        let metadata = fs::symlink_metadata(ancestor)
            .map_err(|error| format!("Parent del workspace non verificabile: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err("Parent del workspace attraversa un symlink o un path ambiguo".into());
        }
    }

    fs::canonicalize(parent)
        .map_err(|error| format!("Parent del workspace non verificabile: {error}"))
}

fn directory_is_empty(path: &Path) -> Result<bool, String> {
    let mut entries = fs::read_dir(path)
        .map_err(|error| format!("Directory workspace non leggibile: {error}"))?;
    match entries.next() {
        None => Ok(true),
        Some(Ok(_)) => Ok(false),
        Some(Err(error)) => Err(format!("Directory workspace non verificabile: {error}")),
    }
}

fn cleanup_new_empty_target(path: &Path) {
    let _ = fs::remove_dir(path);
}

fn initialize(requested: &Path) -> Result<InitializedWorkspace, String> {
    validate_shape(requested)?;
    let current = std::env::current_dir()
        .map_err(|error| format!("Directory corrente non verificabile: {error}"))?;
    let resolved = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        current.join(requested)
    };
    let parent = resolved
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or("Percorso workspace senza parent")?;
    let canonical_parent = real_canonical_parent(parent)?;
    let name = resolved
        .file_name()
        .ok_or("Nome directory workspace non valido")?;
    let target = canonical_parent.join(name);

    let created = match fs::symlink_metadata(&target) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&target)
                .map_err(|error| format!("Creazione workspace fallita: {error}"))?;
            true
        }
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err("Il target workspace è un symlink".into())
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err("Il target workspace non è una directory".into())
        }
        Ok(_) if directory_is_empty(&target)? => false,
        Ok(_) => return Err("La directory workspace esistente non è vuota".into()),
        Err(error) => return Err(format!("Target workspace non verificabile: {error}")),
    };

    let mut inspector_created = false;
    let complete = (|| {
        let metadata = fs::symlink_metadata(&target)
            .map_err(|error| format!("Workspace creato non verificabile: {error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("Workspace creato non è una directory reale".into());
        }
        let canonical_target = fs::canonicalize(&target)
            .map_err(|error| format!("Workspace creato non verificabile: {error}"))?;
        if canonical_target.parent() != Some(canonical_parent.as_path())
            || canonical_target != target
        {
            return Err("Workspace creato fuori dal parent verificato".into());
        }
        let inspector = canonical_target.join("inspector");
        fs::create_dir(&inspector)
            .map_err(|error| format!("Creazione workspace non completata: {error}"))?;
        inspector_created = true;
        let inspector_metadata = fs::symlink_metadata(&inspector)
            .map_err(|error| format!("Directory workspace non verificabile: {error}"))?;
        if inspector_metadata.file_type().is_symlink() || !inspector_metadata.is_dir() {
            return Err("Directory workspace non è una directory reale".into());
        }
        let canonical_inspector = fs::canonicalize(&inspector)
            .map_err(|error| format!("Directory workspace non verificabile: {error}"))?;
        if canonical_inspector.parent() != Some(canonical_target.as_path()) {
            return Err("Directory workspace fuori dal target verificato".into());
        }
        let entries = fs::read_dir(&canonical_target)
            .map_err(|error| format!("Verifica workspace creato: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Verifica workspace creato: {error}"))?;
        if entries.len() != 1 || entries[0].file_name() != "inspector" {
            return Err("Il workspace creato contiene elementi inattesi".into());
        }
        Ok(canonical_target)
    })();

    match complete {
        Ok(path) => Ok(InitializedWorkspace { path, created }),
        Err(error) => {
            if inspector_created {
                cleanup_new_empty_target(&target.join("inspector"));
            }
            if created {
                cleanup_new_empty_target(&target);
            }
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "antiskill-init-unit-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn cleanup_never_removes_a_nonempty_target() {
        let root = temporary("cleanup");
        let target = root.join("workspace");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("preexisting"), b"keep").unwrap();
        cleanup_new_empty_target(&target);
        assert_eq!(fs::read(target.join("preexisting")).unwrap(), b"keep");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn production_does_not_use_subprocess_or_recursive_creation() {
        let source = include_str!("init.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(!production.contains("Command::new"));
        assert!(!production.contains("create_dir_all"));
        assert!(!production.contains("workspace-state"));
        assert!(!production.contains("COLLECTION"));
    }
}
