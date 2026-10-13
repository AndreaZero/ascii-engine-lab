# R7 — pietra asciutta e bagnata

Provino del 13 ottobre 2026: un blocco di pietra lavorato, con frattura ramificata incisa, bordi irregolari, scheggiature e materiale locale. Camera e luce osservano la stessa forma asciutta/bagnata nelle tre visuali. L'immagine nasce dalle query semantiche e dai caratteri; non esiste una scena a mesh renderizzata e poi convertita.

È una prova isolata di forma, superficie e luce. Cortile e Material Room mantengono il proprio aspetto. La qualità finale delle reference, un editor di materiali o una simulazione di bagnatura non sono risultati di questa tranche.

## Avvio e confronto

```sh
cargo run --locked --release --example stone_study -- --window --variant dry --camera third
```

Su Windows: [Avvia-pietra.cmd](Avvia-pietra.cmd), che compila se manca l'eseguibile. Dopo modifiche al codice usare Cargo per ricompilare: il launcher non controlla se il file esistente è vecchio.

| Tasto | Funzione |
| --- | --- |
| 1 / 2 / 3 | Prima persona / terza persona / dall'alto |
| V | Asciutta / bagnata |
| B | Box iniziale / forma incisa con materiale semplice |
| O | Percorso camera / percorso luce |
| Space / R | Pausa / riparti |
| H / Esc | Etichette / chiudi |

Le visuali sono osservatori del provino, non un controller di gioco con collisione o inseguimento del personaggio. La finestra usa un percorso continuo avanti/indietro. Le catture usano un singolo percorso registrato con passo simulato 1/30 s.

```sh
# Quattro passi statici: box, forma, materiale asciutto, materiale bagnato.
cargo run --locked --release --example stone_study -- --variant all --camera all --path static --frames 2 --out evidence/stone-static
# Sei sequenze abbinate: camera mobile per 3 s, poi luce mobile per 3 s.
cargo run --locked --release --example stone_study -- --variant pair --camera all --path showcase --frames 180 --out evidence/stone-motion
# Solo composizione CPU, senza inizializzare la GPU.
cargo run --locked --release --example stone_study -- --benchmark --variant pair --camera all --workers 8 --frames 180 --out evidence/stone-cpu
```

`--path static|orbit|light|approach|showcase` separa le cause del movimento. `--grid economy|balanced|detail` seleziona 120×68, 160×90 o 240×135 celle; l'output headless rimane 1920×1080. Default: 8 worker, limitati alla capacità disponibile; `--workers auto|N` permette di cambiarli esplicitamente. `--capture-every 0` salva solo primo e ultimo frame. Cartelle distinte evitano di sovrascrivere prove precedenti.

Dry e wet hanno identici intersezioni, camera, luce e tick. Baseline e shape usano entrambe il vecchio shading; dry e wet usano entrambe il nuovo. Non confrontare shape e dry come se cambiasse solo il pigmento: cambiano insieme contratto e risposta luminosa.

## Implementazione e confini

| Parte | Implementato | Approssimato / mancante |
| --- | --- | --- |
| Forma | Intersezione analitica di un volume convesso smussato, meno cavità per crepe e due scheggiature. Normali, visibilità e ombra seguono la superficie incisa. | Forma disegnata a mano, soprattutto planare; crepe poco profonde e finite, non erosione, frattura fisica o macigno organico. |
| Superficie | Aggregati, pori, inclusioni minerali, usura ai bordi e variazione locale con coordinate solidali alla pietra. Micro-normali con derivate analitiche. | Campo procedurale originale, non scansione o campione di roccia misurato. I dettagli fini non cambiano la sagoma. |
| Asciutto/bagnato | Zone umide persistenti scuriscono il colore, riducono la rugosità e modificano il riflesso della luce secondo il punto di vista. | Un solo strato artistico; niente pellicola d'acqua fisica, gocce, assorbimento, evaporazione o riflesso completo dell'ambiente sulla pietra. |
| Luce | Diffuso Lambert, speculare GGX/Smith/Schlick, sorgente puntuale, ombra geometrica, albedo/normal/roughness/F0/occlusion locali. | Luce e ambiente non calibrati; F0 scalare, una sorgente, nessun metallo colorato/anisotropia/GI. Floor di rugosità e limite speculare stabilizzano il segnale. |
| Dettaglio e tempo | Frequenze fini attenuate secondo l'impronta della cella, famiglie di glifi condivise con la chiave history, test a stato fermo e camera mobile. | Impronta approssimata, una query geometrica centrale per cella: non è integrazione esatta né antialiasing dei bordi. Popping e scalettature restano possibili. |

La formula speculare si basa sul modello descritto nella [documentazione primaria di Filament](https://google.github.io/filament/main/filament.html). L'uso della formula non rende questa illuminazione fisicamente calibrata: attenuazione, ambiente, bagnatura e compressione del segnale restano dichiaratamente artistici.

Il campione occupa un volume nominale 2,60×1,90×1,72 unità, centrato a (0; 1,05; 0); pavimento a y≈0,10. Fonte delle proprietà in [stone_surface.rs](examples/support/stone_surface.rs), geometria in [stone_geometry.rs](examples/support/stone_geometry.rs), camera e luce in [stone_study.rs](examples/stone_study.rs). Seed/hash procedurali fissi nel sorgente, nessun rumore dipendente dal frame. Atlas font8x8 incorporato, soli caratteri ASCII stampabili. Per i materiali nuovi la luce RGB lineare è (5; 4,7; 4,2); la traiettoria è registrata nei JSON.

### Contratto riutilizzabile

`Scene::local_materials()` attiva il percorso opzionale. `material_sample(hit, footprint)` può restituire albedo lineare, normale di shading, rugosità percettiva, F0 scalare e occlusione locale. `None` conserva lo shading precedente per quella superficie; il default `false` evita anche le nuove query dell'impronta. Il compositore separa normale geometrica e decorativa per non alterare offset e visibilità delle ombre.

`glyph_sample` e `glyph_sample_key` ricevono la stessa impronta. La chiave deve descrivere le regioni strutturali filtrate, indipendentemente dalla luminosità: campionare il solo glifo a luce massima non copre i cambi delle famiglie scure. Questo difetto è emerso in review ed è stato corretto condividendo la classificazione.

L'impronta primaria deriva da raggi camera vicini proiettati sul piano tangente geometrico. Per riflessi resta una stima conica dalla lunghezza del percorso, senza differenziali della curvatura. Il provino pietra usa stage Light; non dimostra ogni combinazione fra nuovi materiali e acqua. Il JSON di Material Room non espone ancora queste proprietà. L'API è sperimentale, senza un nuovo formato universale o dipendenze esterne.

## Verifiche e misure

Consegna verificata: [sintesi versionata](evidence/r7/summary.md), dati grezzi e hash locali in `evidence/r7/manifest.json`.

| Controllo | Risultato |
| --- | --- |
| fmt / test workspace all-targets / clippy / release workspace + esempio | PASS; 114 esecuzioni test incluse copie storiche, di cui 14 del provino. Ripetuti i 14 dopo la sola correzione della camera. Warning ambientali della cache hardlink, nessun lint di codice aperto. |
| Client esistenti | 24 PNG cortile e 12 PNG Material Room identiche a R6, Vulkan/DX12. Test storici di parità celle/history PASS. |
| Pietra sui due backend | 24 coppie PNG identiche: 4 varianti × 3 viste × 2 frame. Nessun cambiamento a stato fermo. |
| Matrice | 72 casi, 9.774 frame; pietra entro i limiti dell'inquadratura, camera/luce/visibilità dry-wet identiche. Statici identici; percorsi dinamici cambiano l'immagine. |
| Determinismo / regressioni | 1/8 worker identici nel test; reset riproducibile; normale, rugosità, ombre, campionamento e cambio di chiave filtrata verificati. |
| Runtime | Finestra Vulkan avviata e chiusa con successo; esecuzioni headless Vulkan/DX12 PASS. |
| Archivi | 6/6 hash della baseline R6 preservati; project.godot conserva l'hash storico. |
| Filmato | 1.080 PNG GPU abbinati; copertura verificata in ogni frame, 18 s / 2560×1080 / 30, 540 frame codificati; decodifica completa PASS. |

Protocollo: Windows x64, Ryzen 7 3800X identificato nelle tranche precedenti; AMD Radeon RX 6700 XT rilevata a ogni esecuzione GPU, Vulkan driver 26.8.1 e DX12. Matrice 3 griglie × 4 percorsi × 3 visuali × 2 materiali, output fisso 1080p, 8 worker; 180 frame dinamici e 3 statici, dopo 12 warmup esclusi. Benchmark CPU separato: 3 ripetizioni da 180 frame per caso a 240×135, ordine worker 1/8, 8/1, 1/8. Budget esplorativo dichiarato prima della prova: p95 CPU ≤16,7 ms con 8 worker sull'host corrente, senza ridurre di nascosto la densità.

| Materiale / vista | CPU p50, 1 worker | CPU p50, 8 worker | Peggior p95, 8 worker |
| --- | ---: | ---: | ---: |
| Asciutta / prima | 17,42 ms | 3,68 ms | 4,82 ms |
| Asciutta / terza | 19,51 ms | 3,93 ms | 5,07 ms |
| Asciutta / alto | 23,78 ms | 5,02 ms | 6,11 ms |
| Bagnata / prima | 17,04 ms | 3,66 ms | 5,13 ms |
| Bagnata / terza | 19,69 ms | 4,13 ms | 5,14 ms |
| Bagnata / alto | 23,29 ms | 4,79 ms | 6,15 ms |

P50: mediana dei tre p50, non mediana di tutti i campioni aggregati. P95: massimo dei tre run, senza scartare code. Budget CPU rispettato nei sei casi a 8 worker. È l'effetto del pool già presente su questo nuovo contenuto, non una nuova ottimizzazione a confronto con R6.

Nella matrice GPU dinamica, p50 CPU per economy/balanced/detail rispettivamente 1,00–1,57 / 1,49–2,30 / 2,94–4,75 ms; peggiori p95 3,43 / 13,90 / 5,88 ms. Il picco balanced resta incluso: questi passaggi non dimostrano una scala perfettamente monotona né sostituiscono le ripetizioni CPU. A detail submission+attesa p50 0,89–0,98 ms, peggior p95 1,33 ms. Working set massimo dei processi pietra: CPU-only 17,52–18,56 MiB; matrice Vulkan 246,48–255,21 MiB; cattura completa 255,04 MiB; runtime statici/finestra su due backend 214,90–324,92 MiB.

Il timer CPU include solo `Composer::compose`; preparazione della piccola scena/camera, conteggio di copertura, confronto delle celle, GPU e I/O sono esterni. Submission+attesa è misurata separatamente, non è un timestamp GPU. Il picco working set del processo è letto con Windows GetProcessMemoryInfo durante la vita del processo: include driver/host e non è VRAM dedicata. Nessuna garanzia FPS, di saturazione hardware o di prestazioni su un altro processore.

La prima iterazione aveva fasce di trattini che ricordavano il legno: sostituite con una grana più irregolare. Le catture hanno rilevato un contatto col pavimento da correggere e un taglio della pietra durante l'avvicinamento in terza persona. L'avvicinamento definitivo passa da 1,32× a 1× la distanza base; il fallimento iniziale resta negli archivi locali. Nessuna riduzione della griglia per superare il controllo.

Le metriche di stabilità contano glifi cambiati fra identità abbinate nella stessa cella schermo, sull'intera immagine incluso il pavimento. Statico: zero cambi. In orbita cambia lo 0,49–1,52% degli abbinati; luce mobile 0,42–0,97%; avvicinamento 0,20–0,67%. Nei percorsi dinamici gli abbinati coprono il 48,17–98,98% degli slot, secondo vista/percorso/griglia. Questi contatori escludono disocclusioni/identità non abbinate e includono cambi motivati di luce o dettaglio: non sono una percentuale di sfarfallio né una certificazione percettiva.

Review della regia e di uno specialista su frame rappresentativi: massa, crepa e scheggiature leggibili; fasce simili al legno eliminate. Restano superfici ampie planari, isole di glifi ripetuti e fratture a tratti troppo nette. Alcune facce bagnate diventano quasi nere e perdono struttura, mentre il riflesso non sempre emerge chiaramente: è un limite artistico aperto, non una qualità finale promossa dai test. Nessun taglio/buco inatteso nei campioni controllati; i singoli PNG non certificano da soli fluidità o assenza di scintillio.

## Filmato e riproduzione

Filmato locale: `evidence/r7/video/pietra-asciutta-bagnata-tre-visuali.mp4`. Due campioni affiancati, 6 secondi per vista: prima camera mobile/luce ferma, poi camera ferma/luce mobile. I margini laterali delle catture 1080p vengono ritagliati senza tagliare la pietra; nessun ridimensionamento, interpolazione, denoising o miglioramento cromatico. Aggiunte soltanto etichette. I 30 frame/s della riproduzione sono una cadenza simulata, non un benchmark.

`scripts/verify-r7.py --phase runtime|matrix|cpu|motion` usa eseguibili release già compilati e raccoglie log/provenienza/memoria. Eseguire le fasi in quest'ordine, una alla volta. `scripts/encode-r7.py` richiede Python e ffmpeg/ffprobe nei percorsi Windows indicati nel file; verifica abbinamento e copertura di tutti i frame prima della codifica. `scripts/finalize-r7.py` verifica parità, baseline, matrice e filmato.

Le verifiche storiche del finalizer richiedono PNG e manifest locali R6, esclusi dal repository pubblico. Da un clone sono riproducibili test Rust, eseguibile, nuove catture e misure con le CLI sopra; non l'intero archivio pregresso. Non compilare, misurare e codificare in parallelo. Media, eseguibili, reference private e dati grezzi non sono pubblicati con il codice.

Passo successivo possibile: un campione di metallo e una porzione di tessuto con risposta e grammatica proprie, poi confronto in un ambiente comune. Prima preservare il presente provino e assegnare una sola tranche con ownership disgiunte; non estendere automaticamente gameplay, editor, fuoco o distruzione.
