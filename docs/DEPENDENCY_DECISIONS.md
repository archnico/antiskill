# Decisioni sulle dipendenze

## shakmaty

- Versione esatta: `0.30.1`.
- Licenza dichiarata dal progetto: `GPL-3.0-or-later`.
- Uso previsto: parsing FEN e SAN, verifica di legalità, applicazione delle
  mosse e canonicalizzazione delle posizioni di scacchi standard.
- La dipendenza è confinata dietro un adattatore interno di Inspector.
- Nessun tipo, hash o formato interno di Shakmaty entrerà nei futuri schemi
  persistenti di Antiskill.
- Il sorgente Antiskill resta **EUPL-1.2**. Le obbligazioni delle dipendenze
  vengono gestite separatamente nel packaging e nei notices degli eventuali binari.

Questa decisione non introduce formati persistenti e non modifica la licenza
del repository.
