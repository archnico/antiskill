# Security

## Segnalazioni sensibili

Non aprire una issue pubblica per una vulnerabilità non ancora corretta, una credenziale esposta o un problema che potrebbe mettere a rischio dati di terzi.

Nel repository pubblico usa **Security → Advisories → Report a vulnerability**. La segnalazione resta privata tra reporter e maintainer.

Includi solo ciò che serve a verificare il problema:

- versione, commit o ambiente rilevante;
- comportamento osservato e impatto plausibile;
- riproduzione minima o proof of concept;
- eventuali mitigazioni già note.

Non allegare password, token, corpus privati o dati personali non necessari. Quando possibile usa fixture sintetiche.

## Bug non sensibili

Per crash, errori di parsing, problemi di portabilità o altri bug che non richiedono embargo, usa normalmente il template **Bug** nelle issue pubbliche.

## Versioni

Prima della stabilizzazione delle release, il ramo `master` corrente è il riferimento supportato. Snapshot storici e branch di sviluppo non ricevono automaticamente correzioni di sicurezza.

---

## English

Please do **not** open a public issue for an unpatched vulnerability, exposed credential, or security problem involving third-party data. Use **Security → Advisories → Report a vulnerability** in the public repository and provide the smallest useful reproduction. Avoid credentials, private corpora, and unnecessary personal data.
