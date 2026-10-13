# R5 — figura, mantello e movimento

13 ottobre 2026. Provino implementato, integrato nel cortile, compilato e verificato con wgpu reale. Nessun runtime Godot modificato. Le immagini derivano direttamente da caratteri, senza mesh renderizzate o filtro RGB→ASCII. Non è la grafica definitiva del personaggio.

## Risultato da guardare

- [Confronto R4/R5 nelle tre visuali](evidence/r5/video/confronto-r4-r5.mp4): prima a sinistra, adesso a destra; sei secondi per visuale, 18 secondi totali.
- [Personaggio corrente nelle tre visuali](evidence/r5/video/personaggio-tre-visuali.mp4): inquadrature complete, luce e camera ferme.
- [Personaggio nel cortile](evidence/r5/video/personaggio-nel-cortile.mp4): acqua, luce mobile e movimento della camera, sempre sei secondi per visuale.

Tutti i video sono 1920×1080, H.264, 30 fotogrammi al secondo, 540 frame, senza interpolazione, correzioni colore o rallentamenti. Sono sequenze di catture GPU a passo simulato 1/30 s: **30 fps del video non è una misura del frame rate dell'applicazione**. Il confronto ritaglia soltanto i margini neri laterali a scala pixel nativa; le scritte sono annotazioni video. PNG originali in `evidence/r5/motion/` e `context/`; comandi di codifica in `video/encoding.json`. Decodifica completa dei tre file PASS, metadati in `video/validation.json`.

Le tre visuali sono camere osservatore sullo stesso stato della figura. La prima è ad altezza occhi e osserva il personaggio; non sono ancora implementate mani/corpo del giocatore in prima persona, movimento giocabile o camera con collisioni.

## Cosa cambia

La figura passa da 15 a 27 parti analitiche. Arti orientati fra spalla, gomito e mano e fra anca, ginocchio e caviglia rendono la posa meno rigida; cappuccio, cintura, fibbia, stivali, guanti e lanterna hanno forme distinte. Respirazione e spostamento del peso sono procedurali; gli stivali restano piantati. Non è un ciclo di camminata.

Il mantello ha pieghe più ampie, deformazione laterale e in profondità, ancoraggio alto e orlo mobile. I caratteri si riferiscono a coordinate locali persistenti, condivise da vista diretta, ombre e riflessi. Mantello, bordo e cuoio hanno materiali separati. Il mantello usa una palette blu-verde spenta, la tunica toni caldi; la luce calda modifica naturalmente il colore osservato. Cucitura centrale, cucitura alta stretta e segni guidati dalla piega sostituiscono la precedente trama a chevron uniforme. Dopo la prima cattura, la cucitura alta è stata assottigliata perché appariva come una scaletta.

La review ha trovato un difetto reale del mantenimento temporale dei glifi: nello stesso bin locale, una piega/cucitura o un materiale diverso potevano conservare il vecchio simbolo se la luminosità cambiava poco. `Scene::glyph_history_key` permette al contenuto di distinguere la struttura dalla sola luce. L'hold richiede ora ID, materiale e chiave coerenti; la stabilizzazione di piccoli cambi luminosi rimane attiva all'interno della stessa struttura. Regressioni end-to-end coprono normale, cucitura, materiale, hold luminoso e reset. Le statistiche di corrispondenza rimangono basate sull'ID, come prima.

## Condizioni e costo

Provino isolato: figura senza pavimento, nero vuoto, luce unica fissa `(-2.3, 3.2, -1.3)`, stage Cloth, griglia 240×135, output 1920×1080. Ogni variante usa lo stesso tick e la stessa camera; pose e parametri esatti sono nei `study.json`. Il compositore R5 è comune ai due percorsi; la baseline conserva attore/materiali R4 e chiave strutturale predefinita. Non è una parità visiva con R4: cambia intenzionalmente il contenuto e si corregge la history del nuovo personaggio.

Host Windows x64, Ryzen 7 3800X, RX 6700 XT. Il modello CPU è ripreso dalle prove R3/R4: la nuova interrogazione CIM è stata negata dal sandbox, fatto annotato in `machine.json`. Rust 1.97.1 MSVC, build release senza specializzazione CPU dell'host. Adapter Vulkan effettivo registrato dalle catture: driver AMD 26.8.1. Seconda verifica DX12 sulla stessa scheda; nessun altro hardware fisico provato.

Composizione CPU isolata: 3 ripetizioni, 120 frame per caso dopo 12 warmup, due varianti, tre camere, uno e otto worker; 4.320 frame misurati. Ordine dei blocchi alternato per ripetizione; niente build, codifica video o altro benchmark del laboratorio concorrente. Snapshot misurato separatamente; conteggio geometrico, GPU e file esclusi dal timer. I conteggi fra frame e il carico del desktop possono comunque influenzare lo stato della CPU.

| Worker | Visuale | R4 p50 mediano fra run | R5 p50 mediano fra run |
| --- | --- | ---: | ---: |
| 1 | Prima | 3,35 ms | 9,44 ms |
| 1 | Terza | 9,33 ms | 26,60 ms |
| 1 | Dall'alto | 7,80 ms | 20,29 ms |
| 8 | Prima | 1,10 ms | 1,70 ms |
| 8 | Terza | 1,93 ms | 4,60 ms |
| 8 | Dall'alto | 1,78 ms | 4,08 ms |

Il costo cresce: più parti, orientamenti, copertura e un solver del mantello da 32 intervalli/8 bisezioni invece di 24/6. Non è un'accelerazione a qualità invariata. R5/8 worker ha p95 fra run di 2,53–5,75 ms in prima, 6,25–6,85 in terza, 5,26–17,27 dall'alto. Esistono anomalie anche nella baseline, ad esempio p50 11,83 ms e p95 38,27 nella seconda ripetizione dall'alto/8 worker: sono conservate, senza attribuire una causa non misurata. Snapshot corrente mediano circa 0,0023 ms nei casi con otto worker. Questi numeri non misurano occupazione CPU/GPU, timestamp GPU, present o FPS garantiti.

## Stabilità e verifica

78 esecuzioni di test PASS con `cargo test --all-targets`, incluse copie di test storici; non sono 78 funzionalità indipendenti. Fmt, clippy con errori sulle diagnostiche di codice e build release PASS. Il compilatore segnala soltanto il fallback da hard link a copie nella cache incrementale.

Controlli del cortile: 4 stage × 3 camere × Vulkan/DX12, 24 PNG, 12 coppie identiche fra backend. Ripetizione statica e history senza cambi in tutti i casi. Smoke a finestra Vulkan completato (192 frame richiesti: non benchmark display). Test traversal/ombre mantengono le verifiche su soglie ed emissivi, comprese 230.400 combinazioni raggio/limite. I test di compositore storico usano contenuto R4; quelli delle query/preparazione usano lo stesso contenuto R5 nei due percorsi. Nessun archivio è stato riscritto per far passare una nuova immagine.

Provino: 1.080 PNG, due varianti × tre camere × 180 frame. Zero differenze a stato fermo con history/reset, nessuna figura tagliata dal bordo del fotogramma, movimento presente in tutte le sequenze. Nella figura corrente risultano abbinate circa 93,8% / 96,8% / 95,0% delle celle occupate (prima/terza/alto); cambi del simbolo sulle celle abbinate circa 1,00% / 1,87% / 1,65%. Sono corrispondenze nella stessa cella schermo e bin locale: **non misurano tutto lo sfarfallio** e non devono scendere a zero mentre il tessuto cambia. Il confronto con R4 non permette di proclamare una riduzione universale dello sfarfallio.

Sono stati ispezionati fotogrammi delle tre viste, del confronto codificato, tempi distinti del mantello e immagini del cortile; la sequenza è disponibile per la valutazione umana del movimento. Nessuna pretesa di validazione percettiva continua automatica. La sagoma e il bordo sono più distinguibili nelle catture, ma restano segni ripetuti nelle zone ampie e una leggibilità dipendente da distanza/illuminazione.

Baseline R4: 14/14 hash preservati in `evidence/r5/baseline/`, incluso eseguibile originale. `examples/support/r4_actor.rs` corrisponde al sorgente R4 esclusi test e spazio finale, controllo automatico PASS. Hash di `project.godot` storico invariato. Dati completi, ripetizioni e limiti in `evidence/r5/summary.json`, protocolli in `cpu/protocol.json`, manifest finale in `evidence/r5/manifest.json`.

## Limiti e passo successivo

Il tessuto è una superficie analitica deformata proceduralmente, senza simulazione fisica, auto-collisioni o contatto con il corpo. Il root finding resta approssimato sui contatti radenti/doppi; un solo raggio per cella può perdere dettagli piccoli. La lanterna è emissiva visivamente, ma non aggiunge una seconda luce al cortile. Acqua ancora piana e con un solo riflesso. Nessun fuoco, distruzione o nemico completo introdotto in R5.

Il quadro di queste capacità e del confine fra resa, simulazione e gioco è in [ENGINE_CAPABILITIES.md](../ascii-game/docs/ENGINE_CAPABILITIES.md). Il passo successivo resta separare i contenuti e dimostrare il riuso con un client indipendente; ulteriori effetti arriveranno come prove circoscritte. Gameplay ed editor completo restano sospesi. La direzione visiva del provino può essere corretta dal riscontro dell'utente prima di aumentare la complessità artistica.

## Riproduzione

```powershell
cargo build --locked --release --bin ascii-engine-lab --example character_study
cargo test --locked --all-targets
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
.\scripts\measure-r5.ps1 -Out evidence/my-r5-cpu
.\scripts\capture-r5.ps1 -Out evidence/my-r5-motion
.\target\release\examples\character_study.exe --camera third --variant current --frames 180 --workers 8 --orbit-speed 0.12 --out evidence/my-orbit
```

`scripts/verify-r5.ps1` e `scripts/summarize-r5.py` puntano alle evidenze di questa tranche: per un'altra revisione usare destinazioni nuove. `scripts/encode-r5.py --source PATH --out NEW_PATH` richiede ffmpeg e crea i filmati dalle sequenze. Sul PC corrente `Avvia-laboratorio.cmd` usa la release R5 già ricompilata; 1/2/3 cambiano visuale, F4 seleziona l'animazione.
