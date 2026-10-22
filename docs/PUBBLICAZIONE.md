# Pubblicazione

## Obiettivo

Il repository pubblico deve essere utile a chi arriva oggi, senza trascinare dettagli dell'ambiente di sviluppo o dati non necessari.

## Dentro

Il pubblico può contenere:

- software;
- documentazione;
- fixture sintetiche o chiaramente redistribuibili;
- piccoli benchmark e artefatti di prova;
- protocolli e risultati aggregati di Ricerca;
- metadati necessari a verificare build e provenienza.

## Fuori

Per impostazione predefinita restano fuori:

- corpus privati o personali;
- credenziali e segreti;
- path, hostname e topologia delle macchine di sviluppo;
- identificativi personali non necessari;
- grandi copie di fonti terze;
- materiale con diritti incerti;
- run pesanti o evidenza raw che può essere custodita separatamente.

Vedi [Privacy](PRIVACY.md).

## Storia Git

La storia pubblica deve superare gli stessi criteri del tree corrente. Se la storia di sviluppo contiene dettagli che non servono al pubblico, è preferibile pubblicare una radice verificata piuttosto che riscrivere distruttivamente l'archivio privato.

La storia privata di sviluppo e la storia pubblica possono quindi essere distinte senza cambiare il significato del software.

## Gate

Prima di una pubblicazione o release verificare:

- `tools/check-public-tree.sh` verde;
- nessun segreto rilevato da uno scanner dedicato;
- fixture e dati con diritti chiari;
- nessun dato personale non necessario;
- link documentali validi;
- README/LEGGIMI e CONTRIBUTING/CONTRIBUIRE coerenti nella sostanza;
- nessun requisito di perfezione stilistica dell’inglese: la sua rifinitura è incrementale;
- CI verde sulle piattaforme dichiarate;
- licenza EUPL-1.2 e DCO presenti;
- descrizioni che distinguono ciò che funziona da ciò che è solo previsto o sperimentale.

## Struttura

L'ingresso pubblico deve restare corto:

1. README / LEGGIMI;
2. Presentazione;
3. Mappa e Architettura;
4. Possibilità e Piano;
5. Contribuire;
6. Fonti, Privacy e documentazione tecnica quando serve.

I documenti storici o di acceptance possono restare disponibili, ma non devono dominare il percorso iniziale.

## Dati e licenza

La licenza EUPL copre il software Antiskill, non attribuisce automaticamente diritti sui dati acquisiti dall'utente o da fonti terze.

Un adapter disponibile non implica che il contenuto della fonte possa essere redistribuito in massa.

## Ricerca

La riproducibilità scientifica non richiede di pubblicare ogni corpus o identità. Protocolli, versioni, hash, manifest e risultati aggregati possono fornire verificabilità mantenendo l'evidenza raw in custodia separata quando privacy o diritti lo richiedono.

## Apertura

Non esiste una fase “collaboratori privati” per il repository pubblico: **il primo momento in cui Antiskill viene aperto ai soci coincide con la pubblicazione generale del repository**.

L’inglese completo o stilisticamente rifinito non è una condizione di apertura. L’italiano può restare la lingua di lavoro dei documenti che cambiano rapidamente; l’inglese viene migliorato progressivamente anche con contributi esterni.

## Regola finale

Il pubblico deve mostrare **il progetto**, non l’archeologia del suo ambiente di sviluppo.

## Binari

La pubblicazione del repository sorgente e la pubblicazione di binari sono decisioni separate. La **EUPL-1.2** resta la licenza del sorgente Antiskill; eventuali artefatti ufficiali devono includere in modo riproducibile licenze e notices applicabili ai componenti di terze parti. Vedi [Licenze](LICENZE.md).
