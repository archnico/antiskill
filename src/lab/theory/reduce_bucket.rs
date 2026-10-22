use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::time::Instant;

const RAW_RECORD_BYTES: usize = 12;
const MAGIC: &[u8; 8] = b"ASKTBK01";

#[derive(Debug)]
struct Config {
    input: PathBuf,
    output: PathBuf,
}

impl Config {
    fn from_args<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut input = None;
        let mut output = None;
        let mut args = args.into_iter();

        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--input" => {
                    input = Some(PathBuf::from(
                        args.next()
                            .ok_or_else(|| "Manca il valore di --input".to_owned())?,
                    ));
                }

                "--output" => {
                    output = Some(PathBuf::from(
                        args.next()
                            .ok_or_else(|| "Manca il valore di --output".to_owned())?,
                    ));
                }

                "--help" | "-h" => {
                    return Err("USO: antiskill lab theory reduce-bucket \
                         --input FILE.bin --output FILE.tbk"
                        .to_owned());
                }

                _ => {
                    return Err(format!("Argomento sconosciuto: {argument}"));
                }
            }
        }

        Ok(Self {
            input: input.ok_or_else(|| "Opzione obbligatoria mancante: --input".to_owned())?,
            output: output.ok_or_else(|| "Opzione obbligatoria mancante: --output".to_owned())?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RawRecord {
    hash: [u8; 10],
    move_code: u16,
    position_marker: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Stats {
    raw_records: u64,
    positions: u64,
    position_moves: u64,
    position_occurrences: u64,
}

fn bucket_from_path(path: &Path) -> Result<u8, String> {
    let stem = path
        .file_stem()
        .and_then(OsStr::to_str)
        .ok_or_else(|| format!("Nome bucket non valido: {}", path.display()))?;

    if stem.len() != 2 {
        return Err(format!(
            "Il nome del bucket deve essere esadecimale a due cifre: {stem}"
        ));
    }

    u8::from_str_radix(stem, 16).map_err(|_| format!("Bucket esadecimale non valido: {stem}"))
}

fn parse_records(bytes: &[u8]) -> Result<Vec<RawRecord>, String> {
    if !bytes.len().is_multiple_of(RAW_RECORD_BYTES) {
        return Err(format!(
            "Dimensione bucket non multipla di {RAW_RECORD_BYTES}: {} byte",
            bytes.len()
        ));
    }

    let mut records = Vec::with_capacity(bytes.len() / RAW_RECORD_BYTES);

    for chunk in bytes.as_chunks::<RAW_RECORD_BYTES>().0 {
        let mut hash = [0_u8; 10];
        hash.copy_from_slice(&chunk[..10]);

        let packed_move = u16::from_le_bytes([chunk[10], chunk[11]]);

        records.push(RawRecord {
            hash,
            move_code: packed_move & 0x7fff,
            position_marker: packed_move & 0x8000 != 0,
        });
    }

    Ok(records)
}

fn sort_records(records: &mut [RawRecord]) {
    records.sort_unstable_by(|left, right| {
        left.hash
            .cmp(&right.hash)
            .then_with(|| left.move_code.cmp(&right.move_code))
            .then_with(|| left.position_marker.cmp(&right.position_marker))
    });
}

fn calculate_stats(records: &[RawRecord]) -> Stats {
    let mut stats = Stats {
        raw_records: records.len() as u64,
        ..Stats::default()
    };

    let mut previous_hash = None;
    let mut previous_pair = None;

    for record in records {
        if previous_hash != Some(record.hash) {
            stats.positions += 1;
            previous_hash = Some(record.hash);
        }

        let pair = (record.hash, record.move_code);

        if previous_pair != Some(pair) {
            stats.position_moves += 1;
            previous_pair = Some(pair);
        }

        if record.position_marker {
            stats.position_occurrences += 1;
        }
    }

    stats
}

fn write_u16(writer: &mut impl Write, value: u16) -> Result<(), String> {
    writer
        .write_all(&value.to_le_bytes())
        .map_err(|error| format!("Errore scrittura u16: {error}"))
}

fn write_u32(writer: &mut impl Write, value: u32) -> Result<(), String> {
    writer
        .write_all(&value.to_le_bytes())
        .map_err(|error| format!("Errore scrittura u32: {error}"))
}

fn write_u64(writer: &mut impl Write, value: u64) -> Result<(), String> {
    writer
        .write_all(&value.to_le_bytes())
        .map_err(|error| format!("Errore scrittura u64: {error}"))
}

fn write_reduced_file(
    records: &[RawRecord],
    stats: Stats,
    bucket: u8,
    output: &Path,
) -> Result<(), String> {
    if output.exists() {
        return Err(format!("L'output esiste già: {}", output.display()));
    }

    let parent = output.parent().unwrap_or_else(|| Path::new("."));

    fs::create_dir_all(parent)
        .map_err(|error| format!("Impossibile creare {}: {error}", parent.display()))?;

    let file_name = output
        .file_name()
        .unwrap_or_else(|| OsStr::new("bucket.tbk"))
        .to_string_lossy();

    let temporary = parent.join(format!(".{file_name}.tmp-{}", process::id()));

    let result = (|| {
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| {
                format!(
                    "Impossibile creare il temporaneo {}: {error}",
                    temporary.display()
                )
            })?;

        let mut writer = BufWriter::with_capacity(1024 * 1024, file);

        writer
            .write_all(MAGIC)
            .map_err(|error| format!("Errore scrittura magic: {error}"))?;

        writer
            .write_all(&[bucket])
            .and_then(|_| writer.write_all(&[0_u8; 7]))
            .map_err(|error| format!("Errore scrittura intestazione: {error}"))?;

        write_u64(&mut writer, stats.raw_records)?;
        write_u64(&mut writer, stats.positions)?;
        write_u64(&mut writer, stats.position_moves)?;
        write_u64(&mut writer, stats.position_occurrences)?;

        let mut position_start = 0_usize;
        let mut written_move_counts = 0_u64;
        let mut written_position_counts = 0_u64;

        while position_start < records.len() {
            let hash = records[position_start].hash;
            let mut position_end = position_start + 1;

            while position_end < records.len() && records[position_end].hash == hash {
                position_end += 1;
            }

            let position_records = &records[position_start..position_end];

            let position_count = position_records
                .iter()
                .filter(|record| record.position_marker)
                .count();

            if position_count == 0 {
                return Err(format!(
                    "Posizione senza alcun marcatore nel bucket {bucket:02x}"
                ));
            }

            let position_count = u32::try_from(position_count)
                .map_err(|_| "Conteggio posizione superiore a u32".to_owned())?;

            let mut distinct_moves = 0_usize;
            let mut cursor = 0_usize;

            while cursor < position_records.len() {
                distinct_moves += 1;
                let move_code = position_records[cursor].move_code;
                cursor += 1;

                while cursor < position_records.len()
                    && position_records[cursor].move_code == move_code
                {
                    cursor += 1;
                }
            }

            let distinct_moves = u16::try_from(distinct_moves)
                .map_err(|_| "Troppe mosse distinte per una posizione".to_owned())?;

            writer
                .write_all(&hash)
                .map_err(|error| format!("Errore scrittura hash: {error}"))?;

            write_u32(&mut writer, position_count)?;
            write_u16(&mut writer, distinct_moves)?;

            written_position_counts += u64::from(position_count);

            let mut move_start = 0_usize;

            while move_start < position_records.len() {
                let move_code = position_records[move_start].move_code;
                let mut move_end = move_start + 1;

                while move_end < position_records.len()
                    && position_records[move_end].move_code == move_code
                {
                    move_end += 1;
                }

                let move_count = u32::try_from(move_end - move_start)
                    .map_err(|_| "Conteggio mossa superiore a u32".to_owned())?;

                write_u16(&mut writer, move_code)?;
                write_u32(&mut writer, move_count)?;

                written_move_counts += u64::from(move_count);
                move_start = move_end;
            }

            position_start = position_end;
        }

        if written_move_counts != stats.raw_records {
            return Err(format!(
                "Verifica fallita: conteggi mosse {written_move_counts}, \
                 record grezzi {}",
                stats.raw_records
            ));
        }

        if written_position_counts != stats.position_occurrences {
            return Err(format!(
                "Verifica fallita: conteggi posizioni {written_position_counts}, \
                 marcatori {}",
                stats.position_occurrences
            ));
        }

        writer
            .flush()
            .map_err(|error| format!("Errore flush: {error}"))?;

        let file = writer
            .into_inner()
            .map_err(|error| format!("Errore chiusura output: {error}"))?;

        file.sync_all()
            .map_err(|error| format!("Errore sync output: {error}"))?;

        fs::rename(&temporary, output).map_err(|error| {
            format!(
                "Impossibile rinominare {} in {}: {error}",
                temporary.display(),
                output.display()
            )
        })?;

        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("Errore sync directory: {error}"))?;

        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }

    result
}

pub fn run<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let config = Config::from_args(args)?;

    if !config.input.is_file() {
        return Err(format!("Bucket non trovato: {}", config.input.display()));
    }

    let bucket = bucket_from_path(&config.input)?;
    let started = Instant::now();

    let input_bytes = fs::read(&config.input)
        .map_err(|error| format!("Impossibile leggere {}: {error}", config.input.display()))?;

    let input_size = input_bytes.len() as u64;
    let mut records = parse_records(&input_bytes)?;
    drop(input_bytes);

    sort_records(&mut records);

    let stats = calculate_stats(&records);

    write_reduced_file(&records, stats, bucket, &config.output)?;

    let output_size = fs::metadata(&config.output)
        .map_err(|error| format!("Impossibile leggere metadata output: {error}"))?
        .len();

    let elapsed = started.elapsed().as_secs_f64();

    println!(
        "\
ANTISKILL THEORY — RIDUZIONE BUCKET
====================================
Bucket: {bucket:02x}
Input:  {input}
Output: {output}

Record grezzi:            {raw_records}
Posizioni distinte:       {positions}
Posizione-mossa distinte: {position_moves}
Presenze posizione:       {position_occurrences}

Dimensione input:  {input_size} byte
Dimensione output: {output_size} byte
Rapporto output/input: {ratio:.4}

Tempo: {elapsed:.3} secondi
",
        input = config.input.display(),
        output = config.output.display(),
        raw_records = stats.raw_records,
        positions = stats.positions,
        position_moves = stats.position_moves,
        position_occurrences = stats.position_occurrences,
        ratio = output_size as f64 / input_size.max(1) as f64,
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(hash_byte: u8, move_code: u16, marker: bool) -> RawRecord {
        RawRecord {
            hash: [hash_byte; 10],
            move_code,
            position_marker: marker,
        }
    }

    #[test]
    fn counts_positions_moves_and_markers() {
        let mut records = vec![
            record(1, 10, true),
            record(1, 11, false),
            record(1, 10, true),
            record(2, 20, true),
        ];

        sort_records(&mut records);

        assert_eq!(
            calculate_stats(&records),
            Stats {
                raw_records: 4,
                positions: 2,
                position_moves: 3,
                position_occurrences: 3,
            }
        );
    }

    #[test]
    fn rejects_truncated_raw_record() {
        let bytes = vec![0_u8; RAW_RECORD_BYTES - 1];
        assert!(parse_records(&bytes).is_err());
    }

    #[test]
    fn parses_marker_without_changing_move_code() {
        let mut bytes = vec![0_u8; RAW_RECORD_BYTES];
        bytes[10..12].copy_from_slice(&(0x8000_u16 | 1234).to_le_bytes());

        let records = parse_records(&bytes).expect("record valido");

        assert_eq!(records[0].move_code, 1234);
        assert!(records[0].position_marker);
    }
}
