# Antiskill Tactics
## La matematica delle idee scacchistiche

**Proposta aperta di collaborazione scientifica · 14 novembre 2026**

> Possiamo riconoscere formalmente un'idea tattica, ritrovarla nelle partite autentiche — anche nelle varianti più oscure — e creare immediatamente un PGN con le partite o gli esercizi più adatti a un certo livello?
>
> **L'ambizione: rendere ricercabili i libri di scacchi che nessuno ha mai scritto.**

**Stato:** proposta di ricerca per una nuova verticale di Antiskill; **non è ancora una funzionalità disponibile**. Questo documento è pensato soprattutto per matematici, informatici teorici e scacchisti interessati alla formalizzazione e alla verifica delle idee scacchistiche.

### 1. Una scena concreta

Un insegnante vuole preparare una lezione su un sacrificio di qualità in **c3** nella **Siciliana Dragone**, magari in una variante secondaria. Chiede tutte le partite disponibili fra giocatori di **1800–2200 Elo** nelle quali il Nero ha *davvero giocato* quel sacrificio, anche attraverso ordini di mosse diversi.

Non cerca soltanto la stringa `...Rxc3`; vuole trovare **il motivo scacchistico**. Deve scartare le catture in c3 che non sacrificano nulla, distinguere la mossa giocata da quella suggerita dal motore e conservare le partite originali.

Altre ricerche potrebbero riguardare:

- **dono greco** (`Bxh7+`, o strutture analoghe sul lato opposto);
- **doppio sacrificio degli alfieri**;
- **sacrificio in h3**;
- **infilata**, inchiodatura e altri motivi geometrici;
- esercizi con un tema specifico e **difficoltà crescente**, senza affidarsi soltanto alle etichette preesistenti.

L'utilità non è solo trovare gli esempi famosi: è far emergere quelli **nascosti in milioni di partite**, comprese aperture per le quali nessuno ha pubblicato un manuale specifico.

### 2. Due risultati direttamente utilizzabili

| Modalità | Output | Garanzia ricercata |
| --- | --- | --- |
| **Partite** | PGN integrali con metadati e provenienza | L'evento tattico cercato è **accaduto nella partita** |
| **Esercizi** | PGN di posizioni con soluzione, varianti e riferimenti | La posizione è **adatta a un esercizio verificabile**, con difficoltà stimata |

Il comando sarà **strutturato e locale**, non in linguaggio naturale. Esempio **puramente progettuale, non ancora eseguibile**:

```bash
antiskill tactics find \
  --theme exchange-sacrifice-c3 \
  --opening sicilian-dragon \
  --player-elo 1800:2200 \
  --played-only \
  --output games \
  --format pgn \
  --out ~/dragone-c3.pgn
```

La verticale termina quando ha prodotto un **file PGN leggibile da altri programmi**. Nessuna pubblicazione online, account Lichess o GUI sono necessari. Lichess può essere un destinatario facoltativo, non una dipendenza.

### 3. Il vero problema matematico

Un motivo tattico non è soltanto una figura sulla scacchiera. È una **struttura di relazioni che si trasforma nel tempo**: attacchi, difese, costrizioni, scambi, minacce e conseguenze.

Per una partita `G` e una sua mossa `t`, possiamo modellare un'occorrenza candidata come

\[
\mathcal O=(G,t,P_t,m_t,T_t),
\]

dove `P_t` è la posizione precedente, `m_t` la mossa *realmente giocata* e `T_t` un albero di continuazioni rilevanti, eventualmente analizzato da un motore. Una rappresentazione naturale è un **grafo dinamico tipizzato**: i nodi descrivono pezzi e case; gli archi descrivono relazioni geometriche e funzionali; le mosse modificano il grafo.

Vogliamo distinguere tre categorie che spesso vengono confuse:

1. **Presenza geometrica:** gli elementi sono allineati o attaccati in una certa configurazione.
2. **Realizzazione tattica:** le condizioni di gioco rendono quella configurazione effettivamente sfruttabile.
3. **Rilevanza della combinazione:** il tema spiega perché la sequenza funziona, invece di comparire soltanto come coincidenza.

#### Infilata: il primo esempio formale

Un attaccante a lunga gittata, un pezzo anteriore e uno posteriore sulla stessa linea definiscono una **configurazione candidata**. Non basta. Occorre verificare che l'attacco sia legale, che lo spostamento del bersaglio anteriore esponga quello posteriore e che vi sia una conseguenza tattica effettiva.

Una buona definizione deve saper rifiutare gli **allineamenti innocui**: i controesempi sono parte del problema, non un fastidio da eliminare.

#### Sacrificio di qualità in c3: un caso più ricco

Una mossa `...Rxc3`, presa isolatamente, non dimostra un sacrificio. Bisogna riconoscere la **cessione materiale effettiva**, gli scambi successivi, il compenso tattico o posizionale, la struttura del Dragone e la distinzione fra opportunità teorica e mossa eseguita.

Tre domande diverse richiedono risposte diverse:

- **È un sacrificio di qualità?** — riconoscimento e bilancio materiale.
- **È un sacrificio corretto?** — valutazione del compenso, dipendente dalla posizione e dall'analisi.
- **È un sacrificio bello o istruttivo?** — giudizio estetico/didattico da studiare anche empiricamente.

#### Doppio sacrificio degli alfieri: composizione nel tempo

Non è sufficiente trovare due alfieri persi. Il tema richiede una **relazione fra due sacrifici**, idealmente con una struttura causale: il primo contribuisce a rendere possibile il secondo e i due cooperano per un obiettivo comune.

Questo apre domande su grafi temporali, condizioni necessarie e sufficienti, relazioni causali e riconoscimento di strutture equivalenti in posizioni diverse.

### 4. Bellezza, difficoltà e correttezza non sono sinonimi

**Correttezza tattica.** Può richiedere una prova forzata, o solo una valutazione probabilistica/posizionale. Il programma deve dichiarare quale tipo di evidenza possiede, senza spacciare la stima di un motore per una dimostrazione.

**Difficoltà.** Un esercizio può essere semplice per un maestro e complicato per un principiante. La profondità del motore non è un misuratore sufficiente. Dove disponibili, i **risultati dei solutori** permettono calibrazioni empiriche; senza di essi si deve riportare una stima e la sua incertezza.

**Bellezza e valore didattico.** Possiamo studiare sorpresa, economia dei mezzi, originalità, coerenza e chiarezza dell'idea. Ma non presumiamo una formula universale: possiamo raccogliere **confronti a coppie** espressi da scacchisti, modellare le preferenze e misurare il disaccordo fra valutatori.

Un obiettivo scientifico interessante è capire **quali caratteristiche della soluzione** predicono il giudizio umano meglio della sola posizione iniziale.

### 5. Come rendere rapide le interrogazioni

«In pochi istanti» non significa rianalizzare milioni di partite ogni volta: significa costruire **un indice delle occorrenze tattiche precalcolato e incrementale**.

Un possibile percorso:

1. **Fonti:** mantenere PGN autentici, hash, provenienza, diritti e identificatori delle partite.
2. **Scoperta a basso costo:** individuare candidati con filtri sulle mosse e relazioni geometriche.
3. **Verifica mirata:** usare legalità, bilanci materiali, analisi già disponibili, motori e controesempi.
4. **Indice tattico:** registrare `game_id`, ply, FEN/posizione, mossa giocata, tema, evidenza, affidabilità, apertura e altri filtri.
5. **Ricerca:** combinare tema, variante, periodo, risultati e fasce Elo; produrre ranking e PGN senza ripetere i calcoli pesanti.
6. **Aggiornamento:** indicizzare nuove partite e nuove definizioni dei temi senza distruggere il corpus originario.

**Le trasposizioni sono essenziali:** riconoscere una variante d'apertura non deve dipendere soltanto dal codice ECO o dall'esatta sequenza testuale. Bisogna stabilire se si interroga un percorso di mosse, una posizione raggiunta o una famiglia di strutture.

**Due livelli da non confondere:** l'Elo dei giocatori che hanno disputato le *partite* e la difficoltà prevista per chi risolve gli *esercizi*.

### 6. Proposta di primo esperimento

**Tema pilota:** sacrificio di qualità `...Rxc3` nella Siciliana Dragone. In alternativa, l'**infilata** è un eccellente primo banco di prova per la formalizzazione geometrica.

Il protocollo iniziale potrebbe essere:

1. Raccogliere **30–50 esempi positivi** e **almeno 50 negativi difficili**: catture non sacrificali, opportunità mai giocate, falsi motivi geometrici, casi controversi e trasposizioni. Queste numerosità sono esplorative, non una garanzia statistica.
2. Esplicitare un **predicato computabile**, con condizioni, controesempi e stati `candidate`, `verified`, `ambiguous`, `rejected`.
3. Ottenere valutazioni da **due annotatori indipendenti**, conservando i disaccordi.
4. Separare addestramento e test **per partita e per famiglie di posizioni**, evitando contaminazioni fra copie o trasposizioni.
5. Misurare **precisione, richiamo, falsi positivi, costo dell'analisi** e qualità delle spiegazioni.
6. Applicare il riconoscitore a un corpus più grande e sottoporre a revisione cieca un campione delle nuove scoperte.

Un algoritmo che trova cento buoni casi e ne spiega il perché è scientificamente più utile di uno che ne dichiara diecimila senza misurare gli errori.

### 7. Che cosa chiediamo ai matematici

Non chiediamo di costruire un'interfaccia, un database scacchistico o un programma completo.

Cerchiamo un contributo a una o più di queste domande:

- **Ontologia e logica:** quali definizioni rendono un motivo tattico computabile?
- **Grafi dinamici:** quali proprietà strutturali restano significative attraverso sequenze diverse?
- **Equivalenza e invarianti:** come riconoscere la stessa idea nonostante le trasposizioni?
- **Causalità e controfattuali:** come distinguere un motivo che *spiega* una combinazione da un dettaglio accidentale?
- **Statistica e validazione:** come stimare gli errori, gestire l'ambiguità e campionare correttamente?
- **Modelli delle preferenze:** quali proprietà determinano bellezza e valore didattico secondo osservatori umani?

**Primo incarico concreto:** produrre una definizione motivata di un singolo tema, un catalogo di controesempi, pseudocodice o un piccolo prototipo e un protocollo di verifica. La scala del lavoro può essere concordata; anche una critica rigorosa alle definizioni iniziali è un contributo utile.

### 8. Regole di integrità

- **Giocato ≠ possibile:** un tema esistente solo in una variante del motore non viene etichettato come evento della partita.
- **Evidenza ≠ inferenza:** conservare livelli di affidabilità, metodo e versione del riconoscitore.
- **Partita ≠ esercizio:** una posizione spettacolare può non essere adatta a un problema con risposta univoca.
- **Raw ≠ indice:** i PGN originali non vengono riscritti; classificazioni e raccolte sono viste derivate ricostruibili.
- **Privacy e diritti:** il repository pubblico contiene specifiche e piccoli esempi redistribuibili; non copie di corpus privati o materiale la cui licenza sia incerta.
- **Architettura:** riusare identità, posizioni, trasposizioni, provenienza, cache e analisi della Base Antiskill, senza creare un secondo nucleo parallelo.
- **Esportazione:** soltanto file locali PGN/manifest; nessuna pubblicazione automatica.

### 9. Riferimenti per iniziare

- **A. Iqbal e M. Yaacob**, *Theme Detection and Evaluation in Chess*, **ICGA Journal**, 31(2), 97–109 (2008). [DOI 10.3233/ICG-2008-31204](https://doi.org/10.3233/ICG-2008-31204). Un precedente diretto sulla formalizzazione di motivi e valutazioni estetiche.
- **[Lichess Open Database](https://database.lichess.org/)**: raccolte aperte di partite, puzzle con temi e rating, e valutazioni. Utile come materiale sperimentale e baseline, purché le etichette siano trattate come dati e non come definizioni definitive.
- **[Base comune e verticali di Antiskill](BASE_E_VERTICALI.md)**: come questa ricerca si innesta sulle capacità condivise del progetto.

---

### In una frase

**Antiskill Tactics vuole rendere le idee scacchistiche oggetti riconoscibili, spiegabili e ricercabili, per scoprire partite dimenticate e comporre esercizi di qualità su qualsiasi tema, anche nei territori in cui nessun autore ha mai pensato di cercare.**

**Collaborazione aperta:** osservazioni, controesempi, formalizzazioni alternative e proposte di esperimenti sono benvenuti.