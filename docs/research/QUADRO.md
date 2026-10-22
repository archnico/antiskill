# Ricerca Antiskill — quadro delle formulazioni

L'“effetto Antiskill” non è ancora una singola quantità canonica. Cambiare baseline cambia la domanda scientifica. Per questo ogni formulazione conserva separatamente **domanda, baseline, popolazione/archivio, quantità stimata, risposta dei dati e stato**.

Questo documento è un indice sintetico. I protocolli e i manifest congelati restano la fonte autorevole per i dettagli operativi.

## Formulazioni correnti

| Formulazione | Domanda | Dati / archivio interrogato | Risposta dei dati | Stato |
|---|---|---|---|---|
| **Casuale fra mosse legali** | Le mosse catastrofiche vengono scelte più spesso di `K/L`, la probabilità ottenuta scegliendo uniformemente una mossa legale? | Pilot pubblico congelato; poi censimento Lichess Rapid gennaio 2025 `<1200` con 900.000 decisioni verificate | Nel censimento: `O=247`, `E=1584,82`, `O/E≈0,156`. Gli umani scelgono le catastrofi molto **meno** della baseline uniforme | risposta forte alla baseline; non è un modello realistico di scelta umana |
| **Scelta umana condizionata alla forza** | Le catastrofi vengono scelte più spesso di quanto preveda un modello di mossa umana per quella fascia di forza? | Stesso insieme congelato di gennaio 2025, 28.668 candidati `primary80`, valutati con Maia‑2 Rapid | `O=247`, `E=235,05`, `O/E=1,051`; `OR=1,065`, CI 95% `[0,920, 1,233]`, `p` one-sided `0,198` | nessuna evidenza confermativa complessiva di eccesso rispetto a Maia‑2 |
| **Differenza relativa fra fasce di forza** | La frequenza/propensione standardizzata alle catastrofi è maggiore fra giocatori deboli che fra giocatori più forti? | Discovery + canary storico sigillato; successivo tentativo su corpus fresco febbraio 2013 | Discovery `R≈1,678`; canary `R≈1,668` ma formalmente inconclusivo. Il tentativo fresco non ha prodotto un test valido per un difetto del sampler | promettente, **non confermata** |
| **Studio principale preregistrato 2025** | Nella popolazione Lichess Rapid 2025 `<1200`, il coefficiente rispetto alla baseline uniforme è positivo secondo il protocollo preregistrato? | Vista full-year 2025 ancora da costruire/fissare secondo il contratto di Fonti/Nucleo | nessun outcome principale autorizzato finché input view e campione non sono congelati | preregistrato, non eseguito |

## Cosa significa ciascuna risposta

### Baseline uniforme `K/L`

È un controllo semplice: misura la distanza dalla scelta casuale fra mosse legali. I dati di gennaio 2025 la respingono nettamente nella direzione opposta all'ipotesi ingenua: i giocatori evitano le mosse catastrofiche molto più di una scelta uniforme.

Questo non decide se esista un effetto Antiskill rispetto al comportamento umano atteso. Dice che **“più catastrofi del caso uniforme” è una formulazione inadeguata come definizione generale dell'effetto**.

### Baseline Maia‑2

Qui la domanda è residuale: dopo aver sostituito il caso uniforme con un modello di scelta umana condizionato alla forza, resta un eccesso di catastrofi?

Nel risultato complessivo congelato di gennaio 2025 la risposta è **no** ai criteri confermativi correnti. Lo strato `<800` mostra invece un eccesso descrittivo (`55` osservate contro `35,07` attese; `O/E≈1,568`), ma Maia‑2 raggruppa le fasce molto basse con risoluzione limitata: il segnale è quindi una pista per una baseline migliore, non una scoperta confermata.

### Formulazione relativa per forza

Questa formulazione non chiede se una popolazione superi una baseline assoluta, ma se il fenomeno sia relativamente più intenso a rating bassi. I risultati di discovery sono interessanti; il canary storico non ha superato il criterio formale di conferma e la prova fresca successiva è rimasta invalida/incompleta per un problema di campionamento.

La domanda resta aperta e merita una replica pulita.

## Registro minimo per i prossimi esperimenti

Ogni nuovo esperimento dovrebbe poter essere riassunto senza prosa lunga con questi campi:

```text
id / versione
formulazione della domanda
estimand o quantità primaria
baseline
popolazione e periodo
archivio/input congelato + identità verificabile
regola di campionamento
analisi/modello e versione
outcome primario
risposta dei dati
stato: esplorativo | preregistrato | confermativo | replica | invalido
limite che impedisce una conclusione più forte
```

Una nuova definizione dell'effetto non riscrive i risultati precedenti: apre una nuova formulazione collegata agli stessi archivi quando appropriato.

## Evidenza pubblica e custodia

Il repository pubblico conserva protocolli, definizioni, conteggi aggregati e risultati necessari a capire e criticare gli esperimenti. Corpus raw, journal voluminosi, pesi di terze parti e identificativi individuali possono restare in custodia separata con manifest e hash quando pubblicarli non è necessario o non è consentito.

Il [pilot esplorativo](pilot-esplorativo-v1.md) e il [protocollo dello studio principale](main-study-protocol-v1.md) restano documenti autonomi. Questo quadro serve a non confondere domande diverse solo perché condividono il nome “Antiskill”.
