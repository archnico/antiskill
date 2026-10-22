# Privacy

Antiskill tratta privacy e pubblicabilità come parte della qualità del software, non come controllo finale cosmetico.

## Regola

Nel repository pubblico entra solo ciò che serve a capire, testare o usare il software.

Per impostazione predefinita **non** entrano:

- corpus privati o personali;
- credenziali, token o chiavi;
- path, hostname e dettagli dell'infrastruttura locale;
- identificativi personali non necessari;
- grandi estratti di fonti terze;
- materiale con diritti di redistribuzione incerti;
- evidenza grezza di ricerca che può restare in custodia separata.

## Fixture

I test pubblici usano preferibilmente fixture:

- sintetiche;
- minime;
- deterministiche;
- chiaramente redistribuibili;
- sufficienti a riprodurre il comportamento.

Un caso reale va ridotto al minimo esempio sicuro quando possibile.

## Ricerca

La riproducibilità non richiede automaticamente di pubblicare identità personali.

Per studi su giocatori o account reali:

- anonimizzare per default quando l'identità non è parte della domanda scientifica;
- pubblicare risultati aggregati quando bastano;
- evitare game ID, username o dettagli reputazionalmente sensibili non necessari;
- conservare separatamente l'evidenza raw quando serve per audit o replica autorizzata;
- documentare protocollo, hash e trasformazioni solo nella misura compatibile con privacy e diritti.

Se l'identità è scientificamente necessaria, la scelta deve essere esplicita e motivata.

## Fonti

Il fatto che un dato sia visibile online non implica automaticamente che sia opportuno copiarlo nel repository.

Ogni adapter deve distinguere:

- accesso;
- uso;
- conservazione;
- redistribuzione;
- attribuzione;
- eventuali limiti o incertezze.

Vedi [Fonti](SOURCES.md).

## Contributi

Prima di allegare dati a una issue o PR, chiediti se il caso può essere spiegato con:

1. una fixture sintetica;
2. un estratto più piccolo;
3. metadati aggregati;
4. un hash o manifest invece del file raw.

Se sì, usa l'opzione meno espositiva.

## Gate pubblico

`tools/check-public-tree.sh` esegue controlli automatici su errori comuni come path personali, email non-esempio, credenziali riconoscibili, symlink problematici e file anomali.

Il gate automatico non sostituisce la revisione umana: contesto, diritti e dati personali possono essere rischiosi anche quando non corrispondono a una regex.

## Principio finale

**Minimo dato pubblico necessario, massima tracciabilità compatibile.**

Privacy e rigore scientifico non sono opposti: un buon protocollo separa identità dell'evidenza, accesso all'evidenza e pubblicazione dell'evidenza.
