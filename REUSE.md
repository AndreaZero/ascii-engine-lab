# R6 — renderer separato e secondo utilizzatore

Aggiornamento R11: [alpha dai dati](ALPHA_DATA.md) verifica materiali locali, conduttori, ambiente e camere dal JSON v2, mantenendo compatibilità v1, ricarica atomica e guida da checkout pulito. I limiti del JSON qui sotto descrivono la consegna R6 storica; il contratto corrente è nella [guida del client](clients/material-room/README.md).

Nota successiva: R7 aggiunge un percorso materiale locale opzionale, documentato in [STONE_STUDY.md](STONE_STUDY.md). Questo report conserva risultati e limiti della consegna R6; il JSON di Material Room non espone ancora i nuovi parametri.

Consegna verificata il 13 ottobre 2026. Il motore ora è un crate distinto, `crates/ascii-render`. Il laboratorio del cortile e la nuova applicazione `clients/material-room` lo usano da due host diversi. Il secondo client non dipende da `ascii-engine-lab`, non importa il personaggio o gli oggetti BLACKWATER e carica una propria scena JSON. È la prima prova concreta di riuso; non è una versione 1 del motore o una dimostrazione della qualità artistica finale.

## Cosa cambia

Gli algoritmi di camera, composizione, history, query e GPU del cortile sono stati estratti senza variazioni. `src/lib.rs` conserva reexport compatibili e l'adattatore `LabScene`; actor.rs e world.rs restano invariati. Il crate renderer contiene test con una scena sintetica propria: non richiede il laboratorio per essere testato.

Material Room ha 7 oggetti analitici, 4 materiali, un pannello rigido mobile e una luce mobile. Ogni frame prepara uno snapshot immutabile. Forme, pigmenti, pattern, trasformazioni e movimento sono dati modificabili; R ricarica il documento, mantenendo la scena precedente se il file è invalido. Il formato v1 è piccolo e validato, con limiti, campi sconosciuti e chiavi duplicate respinti. Il manifest conserva il JSON effettivamente validato, non una rilettura successiva del file.

[Guida completa e schema](clients/material-room/README.md). Su Windows: [Avvia-materiali.cmd](Avvia-materiali.cmd). Il laboratorio originale continua ad avviarsi con Avvia-laboratorio.cmd. La ricarica JSON non richiede ricompilazione; modifiche al codice sì.

## Prove

| Verifica | Risultato |
| --- | --- |
| `cargo check --workspace --offline` | PASS; aggiornato lock per due pacchetti locali, dipendenze esterne bloccate |
| Test workspace/all-targets | 95 esecuzioni PASS, incluse copie storiche; non 95 comportamenti tutti nuovi |
| fmt / clippy all-targets / build release workspace bins | PASS; warning ambientale hardlink cache con fallback a copia, nessun lint di codice aperto |
| Estrazione contro compositore R5 congelato | 360 casi: 3 camere × 4 stage × 3 worker × 2 griglie × 5 tick; celle e statistiche/history identiche |
| Regressione GPU cortile | 24/24 PNG identiche a R5: 12 stage/camere su Vulkan e 12 su DX12 |
| Snapshot/worker secondo client | 1 e 8 worker identici, vecchio snapshot immutato dopo tick nuovi, cambi di stato visibili e reset riproducibile |
| Contenuti | Input invalidi/duplicati/range/riferimenti, geometria e trasformazioni; reload valido/invalido e provenienza dopo rimozione del file PASS |
| Scene reali | Originale e variante JSON: sei catture differiscono dopo soli cambi dei dati; sei coppie originali Vulkan/DX12 identiche |
| Statico | Zero celle diverse a tempo fermo prima/dopo reset, tutte le viste |
| Finestre reali | Lab e secondo client avviati e chiusi con successo su Vulkan |
| Baseline | 14/14 hash R5 preservati; core runtime e contenuto cortile invariati; project.godot conserva hash storico |

La review separata ha individuato e corretto provenienza del manifest, warmup GPU e risveglio per timeout della finestra. Le catture hanno rilevato un taglio nella vista dall'alto del secondo client: inquadratura corretta e sole prove del client ripetute. I test di parità del cortile non sono stati riutilizzati per dichiarare bellezza o realismo dei nuovi campioni.

Il controllo delle dipendenze conferma che il secondo eseguibile importa `ascii-render`, glam, pollster, serde_json e winit. Non importa il package `ascii-engine-lab`. Non è stato simulato un computer senza toolchain/cache: istruzioni per una compilazione da checkout sono fornite, ma installazione pulita e altra macchina fisica restano da verificare.

## Misure circoscritte

Windows x64; GPU rilevata in ogni run: AMD RX 6700 XT, Vulkan driver 26.8.1 e DX12. CPU Ryzen 7 3800X identificata nelle precedenti tranche, non riletta in R6. Stanza originale, 1280×720, griglia160×90, 8 worker, stage Light, 120 frame per vista, 12 warmup CPU+GPU esclusi. Un passaggio sequenziale per backend; non un confronto prestazionale ripetuto. Snapshot e composizione inclusi nel timer CPU; catture fuori dal timer. Scene/tick/camere e distribuzioni complete nei JSON/CSV.

| Camera / Vulkan | CPU p50 | CPU p95 | CPU p99 | Submission+attesa p50 |
| --- | ---: | ---: | ---: | ---: |
| Prima | 1,42 ms | 1,80 ms | 2,05 ms | 0,56 ms |
| Terza | 1,47 ms | 1,89 ms | 2,16 ms | 0,60 ms |
| Alto | 1,30 ms | 1,79 ms | 1,89 ms | 0,49 ms |

I costi appartengono a questi sette oggetti semplici. Non confrontarli con il personaggio R5 come prova di accelerazione: è un altro carico. Submission/attesa non è GPU time; esecuzione offscreen sincronizzata e 30 Hz del filmato non misurano FPS presentati. Questi numeri non certificano prestazioni su altre CPU/GPU. I cambi dei glifi e dei colori sono registrati, ma le metriche locali per ID non esauriscono la stabilità percettiva.

## Filmato e artefatti

- [Filmato nelle tre visuali](evidence/r6/video/material-room-tre-visuali.mp4): 18 s, 1920×1080, 30 fotogrammi/s, 6 s per vista. 540 catture reali wgpu, medesima cadenza simulata; sole etichette aggiunte, nessun miglioramento o interpolazione. Decodifica completa e conteggio frame PASS.
- [Scena originale](clients/material-room/assets/material-room.json) e [variante solo dati](evidence/r6/changed-scene.json).
- [Manifest con hash e risultati](evidence/r6/manifest.json), [misure originali Vulkan](evidence/r6/room-vulkan/run.json), [misure DX12](evidence/r6/room-dx12/run.json).

`scripts/verify-r6.ps1` riproduce controlli runtime con eseguibili release già compilati; `-ClientOnly` ripete la parte del secondo client. `encode-r6.py` codifica le sequenze già catturate; `finalize-r6.py` verifica gli archivi e raccoglie gli hash. Non eseguire build, benchmark o encoder in parallelo. Alcuni confronti richiedono le evidenze locali R5 non incluse nel repository; i test Rust usano sorgenti congelati versionabili. La cartella R6 conserva tutti i dati di questa consegna.

## Implementato, approssimato, mancante

**Implementato:** crate riutilizzabile, due applicazioni, JSON versionato del secondo client, validazione e reload, snapshot coerenti, materiali locali e movimento dei contenuti, catture e verifiche. Il JSON appartiene al client: il cortile resta una scena procedurale e non è stato convertito al formato.

**Approssimato:** forme box/ellissoidi e pattern decorativi semplici; pigmento uniforme per materiale; illuminazione fissa nel compositore; pannello rigido, non stoffa simulata. I ruoli acqua=3/emissivo=8 e i default di camera/stage derivano dal laboratorio. Il secondo client usa materiali >=32 e non espone acqua o emissivi. La separazione non trasforma questi limiti in un'API definitiva.

**Mancante:** qualità di forma/superficie vicina alle reference, risposta locale per ruvidità/bagnato, dettaglio multiscala e campionamento robusto, pipeline artistica completa, editor, altre piattaforme/macchine e gameplay. I modelli attuali restano dimostrativi. Non basta aggiungere caratteri per ottenere realismo: forma, materiale, luce e continuità devono essere progettati insieme.

Prossimo provino: una pietra con bordi irregolari, giunti e crepe, risposta asciutta/bagnata; verificare tre viste e distanze prima di estendere la soluzione. Percorso dettagliato nel documento condiviso `ascii-game/docs/VISUAL_QUALITY_PATH.md`. Nessuna promessa che il metodo attuale raggiunga già quel risultato.
