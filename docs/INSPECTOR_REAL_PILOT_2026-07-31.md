# Antiskill Inspector — primo collaudo reale

Data: 2026-07-31

## Campione

- sorgente osservata: campione PGN privato del pilot (percorso locale omesso)
- dimensione: 40.990 byte
- partite stimate tramite tag `Event`: 59
- workspace temporaneo del pilot: percorso locale omesso

## Identità

- BLAKE3:
  `913d6ee7cf8a6dece754dfd70b5471b531466b90880593a8c010a659d2f3e7fb`
- SHA-256 della sorgente:
  `f1b6292123ff6415aaa866226e5c04160f8b1b1df8f0ae5e22e2d6ad108aad09`
- SHA-256 dell'export:
  `f1b6292123ff6415aaa866226e5c04160f8b1b1df8f0ae5e22e2d6ad108aad09`

## Risultati

1. Il primo import ha creato un nuovo blob di 40.990 byte.
2. Il secondo import ha prodotto un manifest distinto senza aggiungere un blob.
3. Entrambi i manifest indicano lo stesso oggetto content-addressed.
4. L'export è risultato identico byte per byte alla sorgente tramite `cmp`.
5. L'hash della sorgente non è cambiato durante il collaudo.
6. Il workspace contiene un blob e due manifest.
7. Al termine non erano presenti file temporanei.

## Esito

```text
CMP                  PASS
SOURCE_UNCHANGED     PASS
OBJECTS              1
MANIFESTS            2
TEMPORARIES          0
REAL_PILOT_RESULT    PASS
```

## Limite della fase

Il documento è ancora trattato come un unico blob opaco. Il collaudo non
dimostra ancora segmentazione delle singole partite, parsing PGN, ricerca,
deduplicazione scacchistica o ricostruzione selettiva delle occorrenze.
