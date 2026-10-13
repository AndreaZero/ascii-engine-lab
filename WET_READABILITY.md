# R8 — grana leggibile sulla pietra bagnata

Affinamento del provino R7, 13 ottobre 2026. Le mezze ombre della pietra bagnata conservano più struttura grazie a famiglie di caratteri distinguibili anche a bassa luce. Forma, luce, proprietà del materiale e RGB di ogni cella restano identici alla versione precedente. Il nero profondo e le crepe conservano la propria risposta.

La diagnosi ha trovato due riduzioni concomitanti: l'albedo bagnata scurisce il colore; quel colore determina anche glifi più radi, rendendo la superficie ancora meno leggibile. Non era un errore di gamma. R8 interviene sulla seconda parte, senza aumentare la luce della scena. Più inchiostro del glifo aumenta comunque il segnale medio visibile: è una scelta artistica misurata, non una correzione fisica o una nuova riflessione.

## Usare il confronto

```sh
cargo run --locked --release --example stone_study -- --window --variant wet --camera third
cargo run --locked --release --example stone_study -- --variant compare --camera all --frames 180 --out evidence/wet-comparison
cargo run --locked --release --example stone_study -- --variant trio --camera all --frames 180 --out evidence/stone-three-variants
```

Nella finestra `C` alterna **bagnato R7 / bagnato R8**, `V` **asciutto / bagnato R8**, `1/2/3` le viste. Gli altri comandi del [provino pietra](STONE_STUDY.md) restano disponibili. La history viene resettata al cambio di variante: non deve trattenere caratteri della variante precedente. [Avvia-pietra.cmd](Avvia-pietra.cmd) avvia l'eseguibile esistente; ricompilare con Cargo dopo modifiche ai sorgenti.

| Variante | Significato |
| --- | --- |
| `baseline` / `shape` | Box / forma incisa, shading semplice R7 |
| `dry` | Asciutto R7 invariato |
| `wet-r7` | Materiale e grammatica bagnati precedenti |
| `wet` | Stesso materiale, nuova grammatica nelle mezze ombre |
| `compare` | `wet-r7`, `wet` |
| `pair` | `dry`, `wet` (default) |
| `trio` | `dry`, `wet-r7`, `wet` |
| `all` | Tutte le cinque varianti singole |

`--grid`, `--workers`, `--path`, `--capture-every` e `--benchmark` conservano il significato R7. Output headless fisso 1920×1080, griglia default 240×135; `--benchmark` esclude GPU e catture. Il percorso showcase dura 6 secondi per vista: prima camera mobile/luce ferma, poi camera ferma/luce mobile. Cadenza simulata 30 Hz, non FPS misurati.

## Modifica circoscritta

In [stone_surface.rs](examples/support/stone_surface.rs), `wet_glyph` usa la stessa classificazione locale filtrata di `glyph` e `history_key`. Nell'intervallo di illuminazione `[0,025; 0,13)` conserva famiglie granulari `c/o/x` e punteggiatura differenziata. Le cavità mantengono la grammatica precedente. Al di fuori dell'intervallo si usa `glyph` R7, inclusi buio e non-finiti. La soglia di nero del renderer non cambia.

Nessun nuovo parametro di `Scene`, nessun cambio a `sample`, roughness, F0, micro-normali, colore, illuminazione, geometria, camera, shader o font. Il renderer condiviso e i client cortile/Material Room non vengono modificati. La nuova funzione appartiene al contenuto pietra: non viene imposta a tessuto, metallo o altri materiali. La history può mantenere un glifo vicino alle soglie luminose come già in R7; le chiavi delle regioni filtrate restano condivise.

Test dedicato: 81 composizioni abbinate fra R7/R8 su tre visuali e tre percorsi, con 1/8 worker, confronto esatto di colore/ID/flag di tutte le celle e preservazione dei blank. Le catture e i benchmark aggiungono un'impronta deterministica FNV-1a di RGB/alpha/ID/flag per frame, esclusi i glifi. È un controllo diagnostico non crittografico; non sostituisce il confronto esatto nei test.

## Diagnostica della sola pietra

[stone_metrics.rs](examples/support/stone_metrics.rs) considera solo gli hit primari dell'oggetto pietra: esclude pavimento e sfondo. Conta caratteri vuoti e radi, occupazione del font, colori e un proxy del segnale lineare; distingue gruppi per componente dominante della normale geometrica. Questi gruppi non sono facce semantiche: includono anche normali di scheggiature e cavità.

Un carattere rado ha 1–8 texel accesi sui 64 del font8x8; i blank sono separati. L'occupazione dell'atlas non è la copertura esatta dei pixel GPU, soprattutto quando una cella non occupa 8×8 pixel. Il proxy `RGB lineare × copertura × alpha` non è luminanza percettiva né una misura di realismo. Campioni non finiti sono contati ed esclusi dalle medie; gruppi vuoti restituiscono null. Tutta la diagnostica è esterna al timer di composizione CPU.

## Risultati e limiti

Misure finali e verifiche in `evidence/r8/manifest.json`, [sintesi pubblica](evidence/r8/summary.md). Protocollo: stessa macchina Windows x64 di R7, Ryzen 7 3800X identificato nelle tranche precedenti, RX 6700 XT rilevata nei run, Vulkan driver 26.8.1 e DX12. Matrice 3 griglie × 4 percorsi × 3 viste × 2 versioni bagnate, 8 worker, output fisso 1080p, 12 warmup esclusi. CPU separata a 240×135: 180 frame per caso, tre ripetizioni, ordine 1/8, 8/1, 1/8. Budget esplorativo p95 CPU ≤16,7 ms dichiarato prima delle prove.

| Verifica | Risultato |
| --- | --- |
| fmt / test workspace all-targets / clippy / esempio release | PASS; 121 esecuzioni test incluse copie storiche, 21 nel provino. Warning ambientali hardlink cache, nessun lint di codice aperto. |
| Preservazione R7 | 8/8 hash baseline preservati; renderer/shader e geometria invariati. 1.128 PNG identiche agli archivi R7: 48 statiche su due backend, 1.080 in movimento asciutto/bagnato R7. |
| Backend | 30 coppie PNG statiche identiche Vulkan/DX12. Finestra Vulkan avviata/chiusa con successo. |
| Matrice | 72 casi, 9.774 frame; RGB/alpha/ID/flag abbinate, conteggi e camera/luce/visibilità identici. Nessun taglio della pietra; statico senza cambi. |
| Controllo aggiuntivo | 48 casi, 8.640 frame, due ripetizioni mirate dopo picchi nella prima serie; dati iniziali conservati. |
| Video | 1.620 PNG reali wgpu, due montaggi da 18 s/1080p/30 con 540 frame ciascuno; copertura verificata e decodifica completa PASS. |

La quota di **celle della pietra con glifi radi ma non vuoti** scende così nella sequenza showcase. Denominatore: tutte le celle della pietra accumulate sui 180 frame, non solo quelle non vuote.

| Vista | Bagnato R7 | Bagnato R8 | Celle vuote, identiche R7/R8 |
| --- | ---: | ---: | ---: |
| Prima | 16,61% | 8,16% | 4,70% |
| Terza | 26,76% | 12,05% | 19,69% |
| Alto | 35,18% | 10,50% | 14,45% |

Nel gruppo orientato verso +Y della vista dall'alto la quota rada passa dal 46,24% al 5,06%. Il proxy di segnale medio della pietra aumenta circa del 3,9% / 8,4% / 16,9% nelle tre viste, pur con RGB delle celle identici: più texel del carattere vengono accesi. Rimane inferiore all'asciutto nell'aggregato delle sequenze. Questi numeri descrivono la costruzione dell'immagine, non una percentuale di miglioramento artistico.

| Vista / 8 worker | P50 R7 | P50 R8 | Peggior p95 R7 | Peggior p95 R8 |
| --- | ---: | ---: | ---: | ---: |
| Prima | 4,11 ms | 3,92 ms | 7,71 ms | 9,43 ms |
| Terza | 4,09 ms | 4,21 ms | 15,41 ms | 10,07 ms |
| Alto | 5,13 ms | 5,00 ms | 10,45 ms | 11,84 ms |

P50 = mediana dei tre p50 CPU-only; p95 = massimo fra i tre run, senza eliminare code. In seriale i p50 R8 sono 19,56 / 21,38 / 24,00 ms. Le prove CPU ripetute rispettano il budget con 8 worker; differenze e variabilità non dimostrano un'accelerazione. Timer limitato a Composer::compose, con diagnostica, preparazione camera/scena, GPU e I/O esterni.

**Il budget non è rispettato uniformemente in tutta la sessione.** La prima matrice ha 30/72 casi oltre 16,7 ms (15 per variante); i p95 dinamici R8 arrivano a 47,24 ms in economy e 38,24 ms in balanced. I casi statici avevano solo tre campioni, insufficienti per una distribuzione robusta. Un controllo di formattazione si è sovrapposto all'inizio della prima serie; non abbiamo telemetria sufficiente per attribuirgli tutti i picchi. Anche nelle catture complete compaiono p95 R8 di 20,47 ms in terza e 18,25 ms dall'alto; asciutto invariato arriva a 21,41 ms in prima. Non nascondiamo questi passaggi né li attribuiamo con certezza al nuovo materiale o al sistema operativo.

Per chiarire il risultato sono stati ripetuti, senza altri nostri comandi di calcolo concomitanti, economy statico/orbita, balanced avvicinamento e detail showcase: due run da 180 frame, entrambe le varianti e tre viste. Tutti i 48 casi rientrano nel budget; per R8 i massimi p95 sono 2,52 / 4,15 / 14,67 ms nelle tre griglie. Questo controllo conserva gli scostamenti iniziali e non certifica fluidità costante o requisiti hardware. Servirà una campagna di prestazioni in condizioni controllate per una garanzia più forte.

Nel controllo successivo, submission+attesa p50 R8 a detail è 0,94–0,97 ms: non è GPU time. Working set dei processi: CPU-only 17,24–18,13 MiB; prima matrice Vulkan 246,36–258,26 MiB; registrazione completa 258,53 MiB; statici/finestra 216,88–325,05 MiB. Memoria host/driver, non VRAM dedicata.

Contatori di stabilità sull'intera immagine, con identità abbinate nella stessa cella: statico zero cambi; R8 orbita 0,49–1,42%, luce mobile 0,29–0,57%, avvicinamento 0,20–0,60%. Il pavimento è incluso in questi contatori del compositore (diversamente dalle metriche di copertura specifiche della pietra); disocclusioni escluse e cambi motivati inclusi. Non sono una misura completa dello sfarfallio.

Review delle prime catture: recupero netto sulla sommità, maggiore continuità nella parte inferiore destra in prima persona, miglioramento più contenuto su bordi e mezze ombre in terza. Il fianco in ombra profonda resta nero. Crepa distinta e silhouette invariata. Persistono aggregati tipografici ripetitivi e forme planari: è ancora un materiale stilizzato.

Il riflesso della luce non è stato modificato. La sua leggibilità resta dipendente dall'orientamento e dalla rugosità filtrata; il provino non aggiunge riflessi d'ambiente, acqua fisica, illuminazione indiretta o nuovi campioni geometrici. I contatori di cambi glifo e i PNG isolati non certificano assenza di scintillio. La successione completa serve a valutare la resa in movimento; il giudizio artistico dell'utente resta distinto dai PASS automatici.

## Evidenze e riproduzione

Due filmati locali da 18 secondi, ciascuno nelle tre visuali:

- `evidence/r8/video/comparison/bagnato-prima-dopo-tre-visuali.mp4`: R7 a sinistra, R8 a destra.
- `evidence/r8/video/materials/pietra-asciutta-bagnata-r8.mp4`: asciutto a sinistra, bagnato R8 a destra.

Catture reali wgpu, affiancamento con ritaglio dei soli margini laterali controllato su ogni frame, etichette; nessun ridimensionamento, interpolazione, filtro o miglioramento cromatico. Output 2560×1080, 540 frame a 30 frame/s simulati.

Compilare l'esempio release, poi eseguire `scripts/verify-r8.py --phase runtime`, `matrix`, `cpu`, `motion`, una fase alla volta. La fase `control` riproduce il controllo mirato senza sovrascrivere la prima matrice. `scripts/encode-r8.py` richiede ffmpeg/ffprobe nei percorsi Windows dichiarati; `scripts/finalize-r8.py` verifica baseline, immagini, matrice, controllo, diagnostica, video e provenienza. Alcuni confronti richiedono le registrazioni R7 locali, non incluse in Git. Da un clone sono disponibili test Rust e nuove registrazioni tramite le CLI sopra. Eseguire anche fmt e verifiche leggere prima delle misure; non eseguire build, benchmark ed encoder contemporaneamente.

R7 è conservata in `evidence/r8/baseline` e nel commit `886bd0b`. I dati grezzi e i video restano locali; codice, baseline sorgenti, report e sintesi sono versionati. Il prossimo provino potrà verificare la risposta specifica di un altro materiale, senza continuare indefinitamente a ritoccare lo stesso campione.
