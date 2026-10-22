# Formato degli snapshot grezzi

## Obiettivo

Ogni acquisizione deve essere verificabile e riproducibile senza dipendere dal percorso originale o dalla disponibilità futura della sorgente.

Il sistema distingue:

- **oggetto grezzo**: contenuto byte-per-byte identificato dal BLAKE3;
- **manifest di acquisizione**: prova che una determinata sorgente è stata controllata in un dato momento;
- **riferimento latest**: puntatore aggiornabile all'ultimo manifest della sorgente.

Contenuti identici non vengono duplicati. Due controlli effettuati in giorni differenti producono invece due manifest distinti, anche quando puntano allo stesso oggetto.

## Struttura

```text
ROOT/
└── raw/
    ├── objects/
    │   └── blake3/
    │       └── ab/
    │           └── abcdef...
    ├── manifests/
    │   └── torneionline-allin/
    │       └── TIMESTAMP-PID-HASHPREFIX.json
    ├── refs/
    │   └── torneionline-allin.json
    └── tmp/
```

Gli oggetti non hanno estensione: tipo, URL e intestazioni HTTP appartengono al manifest e non al nome fisico.

## Campi del manifest v1

- `schema_version`
- `source_key`
- `source_url`
- `fetched_at_unix_ms`
- `http_status`
- `content_type`
- `content_length_header`
- `etag`
- `last_modified`
- `content_disposition`
- `rights_class`
- `retrieved_by`
- `blake3`
- `bytes`
- `object_path`
- `new_object`

`new_object` indica se l'acquisizione ha aggiunto nuovi byte all'object store. Non indica se sia stato creato un nuovo manifest: ogni acquisizione riuscita crea sempre il proprio manifest.

## Garanzie

1. Il download viene scritto prima in `raw/tmp`.
2. Hash e dimensione vengono calcolati durante la scrittura streaming.
3. Un file vuoto viene rifiutato.
4. L'oggetto è registrato con un hard link nello stesso filesystem, evitando copie incomplete visibili.
5. Il manifest viene scritto solo dopo la registrazione dell'oggetto.
6. Il riferimento `latest` è derivato e può essere ricostruito dai manifest.
7. Nessun parser modifica l'oggetto grezzo.

## Primo connettore

```bash
antiskill sources sync torneionline-allin --root ROOT
```

La prima versione acquisisce soltanto il file. Parsing delle colonne, risoluzione FSI/FIDE e confronto semantico verranno aggiunti in incrementi successivi.
