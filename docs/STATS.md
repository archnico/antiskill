# Antiskill Stats · CLI strutturata

**Stato:** prima verticale implementata nel ramo feat/stats-cli-v1-20261119. Interroga snapshot FSI immutabili e report Rankings locali; non genera rating da fonti secondarie, non certifica report, non pubblica nominativi automaticamente.

## Uso

    antiskill stats --help
    antiskill stats reports
    antiskill stats members --region TOS --year 2026 --by club
    antiskill stats members --region TOS --year 2026 --by club,club-province --limit 30
    antiskill stats members --region TOS --year 2026 --by club,type --under 18
    antiskill stats members --region TOS --year 2026 --club 17029 --by type
    antiskill stats members --club 17029 --year 2026 --by category
    antiskill stats top3 --region TOS --rating standard  # ultimo report disponibile
    antiskill stats top3 --region TOS --month 2026-11 --rating standard
    antiskill stats top3 --region TOS --rating standard  # ultimo report disponibile
    antiskill stats top3 --region TOS --month 2026-11 --rating standard --cohort U16 --sex female
    antiskill stats club --club 17029 --rating standard  # ultimo report disponibile
    antiskill stats club --club 17029 --month 2026-11 --rating standard
    antiskill stats club --club 17029 --month 2026-11 --rating rapid --under 18
    antiskill stats coverage --region TOS --month 2026-11 --by club-province
    antiskill stats coverage --region TOS --month 2026-11 --by type
    antiskill stats members --region TOS --year 2026 --by club --format csv
    antiskill stats members --region TOS --year 2026 --by club --format json

I dati si cercano in una directory indicata con --root oppure ANTISKILL_STATS_ROOT, in alternativa in XDG_DATA_HOME/antiskill. Il report di prova di Firenze Scacchi usa il codice FSI **17029**.

## Semantica

- **members**: legge lo snapshot FSI ufficiale per un anno e uno scope (circolo o regione del circolo). Le dimensioni selezionabili sono club, club-name, club-province, residence-province, region, type, category, birth-year, country. Il raggruppamento multiplo usa --by dim1,dim2[,dim3]. Le righe sono tesseramenti, con conteggio separato degli ID FSI distinti; non si confondono persone e tesseramenti.
- **top3**: consulta i gruppi U08–U18 e absolute/female di un report Rankings già prodotto. Non esegue il calcolo e non presenta classifiche non disponibili. Rating standard, rapid, blitz, italia; il report deve avere esattamente il rating richiesto.
- **club**: consulta il report Rankings per circolo e gli eventuali filtri originali, inclusa la fascia under. Le dimensioni non contemplate dal report non possono essere inferite.
- **coverage**: mostra aggregati delle fonti FSI (tipologie tessera, province del circolo e province di residenza) salvati con il report regionale.
- **reports**: inventario dei report disponibili.

## Garanzie e limiti

1. Lettura locale, niente download impliciti. I manifest FSI sono controllati per source key, HTTP status, dimensione e BLAKE3; una sostituzione della fonte porta a errore.
2. Senza --month si usa il periodo disponibile più recente. Se più report hanno medesimo scope/periodo/rating viene preferito esclusivamente l'unico report `rankings-YYYY-MM-managed` promosso dall'orchestratore dopo validazione atomica. Se il report gestito non esiste oppure vi sono più candidati senza autorità univoca, si richiede --report FILE; nessun report viene scelto arbitrariamente.
3. Le statistiche riportano periodo, fonte, impronte, autorità e stato di certificazione. Un report senza rating FIDE non viene presentato come classifica FIDE.
4. La regione indicata per i tesserati è quella **di affiliazione del circolo**, non la residenza. Per la residenza esiste il raggruppamento residence-province.
5. L'età viene valutata rispetto all'anno richiesto, in convenzione anno solare. Il sesso non è presente nell'export FSI: per statistiche femminili si possono usare solo report che contengano dati compatibili e dichiarino la relativa copertura.
6. Il comando non rende pubbliche automaticamente le classifiche di minori, né redistribuisce CSV FSI o XML FIDE. Ogni report conserva il proprio status e i propri diritti.
7. Nessuna statistica nazionale è derivata da un export locale: è necessario acquisire gli scope previsti e riconciliarli.

## Evoluzione senza proliferazione di sottocomandi

Nuova dimensione: estendere l'elenco dimensioni del motore members e aggiungere test. Nuova metrica: dichiarare unità, autorità e denominatore; aggiungere una vista alla struttura stats senza duplicare import FSI/FIDE, registry identità o deduplicazione delle partite. Per rating, calcolo storico e Top 3 la fonte resta Rankings; Stats è il livello di interrogazione.
