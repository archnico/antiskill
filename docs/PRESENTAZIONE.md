# Presentazione

## In una frase

**Antiskill costruisce archivi scacchistici locali, verificabili e riusabili, sui quali cercare, produrre rapporti, integrare fonti, eseguire analisi e fare ricerca scientifica.**

## Il problema

Le partite di scacchi arrivano da fonti diverse e spesso non coincidono perfettamente:

- la stessa partita può comparire più volte;
- i tag possono essere incompleti o discordanti;
- nomi e identità possono variare;
- una fonte può sparire o cambiare;
- le analisi engine possono essere costose;
- un esperimento scientifico è debole se non è chiaro quali dati abbia usato.

Antiskill prova a risolvere questi problemi senza nascondere l'evidenza originale.

## La scelta

Il progetto costruisce una **Base comune compatta**: capacità stabili che applicazioni diverse possono riusare senza ricostruire ogni volta archivio, identità, provenienza, query o lavori lunghi. Il Nucleo ne è il cuore semantico.

Il percorso conservativo parte da:

```text
PGN e fonti
→ evidenza e provenienza
→ identità e deduplica
→ ricerca ed export
```

Da questa Base si irradiano **verticali**: percorsi end-to-end che attraversano le aree necessarie e terminano in un risultato utilizzabile:

```text
Rapporti · Analisi · Ricerca · Conoscenza · Interoperabilità
```

La **Piattaforma** esegue i lavori; il **Laboratorio** prova idee; **Qualità** controlla tutto. Le aree assegnano responsabilità, mentre [Base e verticali](BASE_E_VERTICALI.md) descrivono riuso e prodotti.

## Cosa funziona già

La superficie più matura è il workflow PGN locale:

```text
init → ingest → doctor → compare/search/duplicates/export
```

Antiskill conserva le occorrenze originali, la provenienza e le identità separate, diagnostica i duplicati in modo non distruttivo e può esportare il PGN originale byte per byte.

## Cosa può diventare

Il progetto non vuole decidere in anticipo un'unica applicazione finale. Tra le direzioni plausibili:

- motore di ricerca di partite, posizioni e trasposizioni;
- dossier giocatore, repertorio, torneo e corpus;
- preparazione contro un avversario;
- archivio di analisi Stockfish e Syzygy riusabili;
- controllo di completezza di tornei e broadcast;
- riconciliazione di dati federali e sistemi torneo;
- ricerca storica e archivi scacchistici;
- strumenti per istruttori e studio;
- integrazioni con En Croissant, Scid, notebook e altri strumenti;
- esperimenti scientifici sul comportamento umano, incluso l'effetto Antiskill.

Una proposta utile non deve per forza entrare nel Nucleo: può vivere come adapter, ricetta, integrazione, prototipo o progetto complementare.

## Metodo

Le regole più importanti sono poche:

1. conservare l'evidenza;
2. rendere esplicita la provenienza;
3. non confondere deduplica con cancellazione;
4. separare dati, analisi e risultati scientifici;
5. usare fixture sintetiche e minimizzare dati personali nel pubblico;
6. tenere il Nucleo piccolo e locale;
7. promuovere un prototipo solo dopo una prova concreta.

## Collaborare

Non serve conoscere tutto Antiskill né programmare in Rust.

Sono utili contributi di:

- giocatori e istruttori;
- arbitri e organizzatori;
- storici e archivisti;
- statistici e data analyst;
- programmatori Rust, Python e GUI;
- tester Windows/macOS/Linux;
- persone che conoscono federazioni, siti o formati scacchistici.

Per una piccola correzione basta una PR focalizzata. Per una nuova applicazione basta spiegare **quale problema risolve** e **come potrebbe usare Antiskill**: il confine tecnico si decide insieme dopo.

Vedi [Contribuire](../CONTRIBUIRE.md) e [Possibilità](POSSIBILITA.md).
