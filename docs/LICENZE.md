# Licenze

## Scelta del progetto

Il codice originale di Antiskill è distribuito con **EUPL-1.2**. Il riferimento normativo è il testo ufficiale in `LICENSE`; `Cargo.toml` usa lo stesso identificatore SPDX.

La scelta è intenzionale: Antiskill può essere usato liberamente anche in attività professionali e commerciali, mentre le modifiche e redistribuzioni del software restano soggette alle condizioni della EUPL. Software, servizi e interfacce indipendenti costruiti attorno ad Antiskill possono mantenere una propria licenza quando restano opere indipendenti.

I contributi accettati nel repository sono distribuiti sotto la stessa **EUPL-1.2** e usano DCO 1.1 (`git commit -s`). Ogni contributor conserva il copyright sul proprio contributo.

## Dati e contenuti esterni

La licenza del software non cambia i diritti sui dati. PGN, database, broadcast, fonti federali, documenti, modelli e altri contenuti importati mantengono licenze e vincoli propri.

Antiskill pubblica software e contratti di interoperabilità; non attribuisce automaticamente diritti di redistribuzione sui corpus usati localmente.

## Dipendenze

Le dipendenze di terze parti mantengono le proprie licenze. Tra le dipendenze dirette attuali, `shakmaty` e `pgn-reader` sono `GPL-3.0-or-later`.

La documentazione ufficiale EUPL include GPL v3 tra le licenze compatibili e distingue il semplice linking/interoperabilità dall'incorporazione di codice. Per questo la presenza di tali dipendenze **non è un blocco alla pubblicazione del sorgente Antiskill sotto EUPL-1.2**.

Riferimenti ufficiali:

- https://interoperable-europe.ec.europa.eu/collection/eupl/eupl-text-eupl-12
- https://interoperable-europe.ec.europa.eu/collection/eupl/faqs

## Artefatti e binari

La pubblicazione di binari è un problema di packaging separato dalla licenza del sorgente. Una release ufficiale deve conservare le licenze applicabili ai componenti inclusi e fornire notices e accesso al sorgente quando richiesto.

Questo requisito non blocca il repository pubblico. Quando verranno distribuiti binari ufficiali, il processo di release dovrà generare in modo riproducibile l'inventario delle dipendenze e i relativi notices invece di affidarsi a documentazione manuale.

## Nuove dipendenze

Una nuova dipendenza diretta va valutata anche per licenza, obblighi di redistribuzione, sicurezza e manutenzione. Il fatto che una crate compili non basta a renderla automaticamente adatta a una release ufficiale.
