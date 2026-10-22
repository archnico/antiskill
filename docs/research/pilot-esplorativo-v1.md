# Ricerca Antiskill — pilot esplorativo v1

Stato: **completato, esplorativo**.

Per il confronto con le formulazioni successive vedi il [quadro delle ipotesi](QUADRO.md).

## Domanda

Quando una posizione offre molte mosse sicure e solo una o due mosse catastrofiche, un giocatore sceglie le mosse catastrofiche più spesso di una baseline uniforme sulle mosse legali?

Per la posizione `i`:

- `L_i` = mosse legali;
- `K_i` = mosse catastrofiche;
- `p_i = K_i / L_i`;
- `Y_i = 1` se la mossa giocata è catastrofica;
- `O = ΣY_i`;
- `E = Σp_i`.

La baseline primaria è la Poisson-binomial condizionata ai singoli `p_i`.

## Campione

Il pilot usa un singolo giocatore e serve a validare protocollo, runner e interpretazione prima di studi di popolazione.

Per privacy, username, game ID e altri identificativi individuali non sono pubblicati: non sono necessari per interpretare il risultato aggregato né per definire il protocollo.

Input congelato:

- reservoir storico: `5000` decisioni;
- seed: `20260726`;
- filtro: `ply >= 16`;
- decisioni analizzate dopo il filtro: `3154`;
- FEN distinti: `3154`;
- partite rappresentate: `2160`.

Il PGN raw e il journal completo restano evidenza di ricerca separata dal repository pubblico.

## Analisi engine

Protocollo finale:

- Stockfish 18;
- un thread per worker;
- `Hash = 64 MiB` per worker;
- `50000` nodi per ogni mossa legale;
- ogni root move analizzata con `searchmoves`;
- prima di ogni analisi: `ucinewgame`, `Clear Hash`, `isready`;
- nessun prefilter nel run finale;
- score atteso derivato dal WDL come `W + D/2` per il lato al tratto.

Test diagnostici hanno verificato l'equivalenza tra processo fresco per mossa e worker persistente completamente resettato sul campione controllato.

## Opportunità

Una mossa è **sicura** quando la perdita di expected score rispetto alla migliore è al massimo `10` punti percentuali.

Una mossa è **catastrofica** quando la perdita è almeno `35` punti percentuali.

Una posizione entra nel campione solo quando:

- la migliore mossa ha expected score `>= 35%`;
- `K ∈ {1,2}`;
- `K/L <= 15%`;
- la frazione di mosse sicure raggiunge la soglia considerata.

Soglie osservate: `60%`, `70%`, `80%`, `90%`. La soglia primaria storicamente dichiarata è `80%`.

## Perché `ply >= 16`

Il pilot storico all-ply era dominato da posizioni iniziali ripetute. Il filtro `MIN_PLY=16` era già il default del prototipo precedente e riduce la contaminazione da aperture senza essere scelto ottimizzando il risultato finale.

## Risultato

| Soglia sicura | Opportunità | Partite | O | E | O/E | intervallo θ 95% | two-sided |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 60% | 85 | 80 | 3 | 3.7521 | 0.7995 | [0.1667, 2.2303] | 0.95727 |
| 70% | 85 | 80 | 3 | 3.7521 | 0.7995 | [0.1667, 2.2303] | 0.95727 |
| **80%** | **85** | **80** | **3** | **3.7521** | **0.7995** | **[0.1667, 2.2303]** | **0.95727** |
| 90% | 77 | 72 | 1 | 2.9409 | 0.3400 | [0.0086, 1.8286] | 0.40237 |

Valori primari non arrotondati:

```text
N                    85
O                    3
E                    3.752133607
O/E                  0.799545089
theta 95% CI         [0.166719139, 2.230322346]
P_uniform(X <= O)    0.478632692665
P_uniform(X >= O)    0.732117987161
two-sided            0.957265385330
```

## Interpretazione

Sono state osservate 3 scelte catastrofiche in 85 opportunità primarie, contro 3.752 attese dalla baseline uniforme condizionata alla posizione.

Il risultato è pienamente compatibile con la baseline e **non fornisce evidenza di un eccesso di mosse catastrofiche**. L'intervallo è ampio: il pilot è inconclusivo sulla dimensione dell'effetto e non dimostra che un eventuale effetto Antiskill sia assente.

Non è consentita generalizzazione a fasce Elo, popolazioni Lichess, FIDE o esseri umani in generale: è un pilot a singolo giocatore.

## Cosa ha validato

Il valore principale del pilot è metodologico:

- protocollo engine deterministico dopo reset completo;
- runner completabile e verificabile;
- filtro che evita la dominanza artificiale delle aperture iniziali;
- definizione esplicita delle opportunità;
- separazione tra risultato esplorativo e studio confermativo.

Lo studio principale è definito in [main-study-protocol-v1.md](main-study-protocol-v1.md).

## Privacy e riproducibilità

L'identità del giocatore e gli ID delle singole partite non sono necessari al documento pubblico. L'evidenza raw originale è custodita separatamente per audit e replica autorizzata.

La versione pubblica conserva metodo, parametri, numerosità e risultati aggregati: ciò che serve per comprendere e criticare il protocollo senza pubblicare dettagli personali superflui.
