# ASCII Engine Lab — prima prova Rust + wgpu

Laboratorio autonomo per un motore di rappresentazione ASCII riutilizzabile. BLACKWATER è il primo utilizzatore previsto; gameplay sospeso. Il progetto Godot nella cartella sorella resta riferimento storico e non viene caricato da questo programma.

Questa è una prova tecnica progressiva, non la grafica finale e non un motore già validato rispetto a Godot. Rust + wgpu è la base richiesta dall'utente; non c'è un benchmark equivalente che ne dimostri superiorità.

## Avvio

Su questa macchina: aprire `Avvia-laboratorio.cmd`, che esegue la build release presente oppure la compila se manca. Per ricompilare dopo modifiche usare Cargo (il launcher non ricompila automaticamente una build già presente):

```powershell
cd C:\Users\andre\Documents\cursor-projects\ascii-game\ascii-engine-lab
cargo run --locked --release
```

Toolchain verificata: Rust 1.97.1 x86_64-pc-windows-msvc. Dipendenze bloccate in Cargo.lock; wgpu 26.0.1, winit 0.30.12. La compilazione iniziale richiede accesso a crates.io e linker MSVC. Non sono necessarie installazioni Godot, font di sistema o asset esterni.

| Controllo | Azione |
| --- | --- |
| 1 / 2 / 3 | Osservatore in prima persona / terza / dall'alto |
| F1 / F2 / F3 / F4 | Core / luce mobile / acqua / tessuto animato |
| Q / E | Rotazione della visuale (yaw in prima, orbita nelle viste esterne) |
| WASD, Z / X | Sposta l'osservatore; Z/X abbassa/alza |
| Spazio | Pausa il tempo di simulazione |
| H | Mostra/nasconde riga diagnostica |
| P | Cattura PNG della stessa pipeline GPU in evidence/ |
| R / Esc | Ripristina vista e tempo / esci |

Sono camere di laboratorio: non c'è ancora un controller giocabile con collisioni, mani in prima persona o inseguimento di un player controllato. Il cambio vista non resetta tempo, luce o posa. Lo spostamento osservatore può attraversare oggetti: limite esplicito della prova.

## Cosa produce l'immagine

`Scene::trace` interroga forme analitiche e restituisce posizione, profondità, ID, materiale e normale. Il compositore sceglie carattere ASCII e colore dal materiale e dalla luce. wgpu riceve solo le celle e disegna i glifi dell'atlas font8x8. Nessuna mesh di mondo, immagine RGB del mondo, texture Godot o filtro ASCII alimenta la scelta dei caratteri. Il triangolo fullscreen è solo il supporto con cui il fragment shader deposita l'inchiostro dei caratteri.

La copertura opaca viene risolta prima dell'inchiostro: gli spazi in una lettera non mostrano l'oggetto dietro. Il nero è sfondo, non un riempimento solido colorato. Font 8×8 incorporato, ASCII stampabile 32–126. Shader di uscita senza antialiasing temporale.

| File | Responsabilità |
| --- | --- |
| src/core.rs | Ray/Hit/GlyphCell, contratto Scene e intersezioni numeriche |
| src/budget.rs | Profili espliciti e risoluzione del budget CPU |
| src/camera.rs | Prospettiva e vista ortografica dall'alto |
| src/frame.rs | Visibilità/luce/riflesso → glifi; memoria conservativa e diagnostica |
| src/gpu.rs, src/glyph.wgsl | Presentazione e catture wgpu, senza conoscenza del contenuto |
| src/world.rs, src/actor.rs | Fixture sostituibile: cortile, luce, acqua, figura e mantello |
| src/main.rs | Finestra, input, CLI e protocollo misure |

Un altro utilizzatore può implementare `Scene` e alimentare lo stesso compositore; un produttore alternativo può produrre `GlyphCell` direttamente. Non sono ancora un SDK stabile o un formato universale di scene.

Il compositore richiede `Scene + Sync` e query deterministiche di uno stato immutabile. L'app prepara `LabScene::snapshot(time, stage)` una volta per composizione, riutilizzando posa e luce per i raggi primari, le ombre e i riflessi. Un altro client può fornire il proprio snapshot tramite `Scene`.

Il pool CPU è persistente. Il budget automatico usa la stima di parallelismo disponibile e il tetto del profilo; quando possibile lascia un'unità fuori dal pool. Non è una riserva di core fisici né un'affinità imposta al sistema operativo. `--workers 1` usa il percorso seriale; un numero esplicito può superare il tetto del profilo, entro la capacità stimata e il limite applicativo di 64. Il numero ottimale dipende dalla macchina e dal contenuto.

## Budget e compatibilità

| Profilo | Output | Griglia caratteri | Tetto worker automatici |
| --- | --- | --- | --- |
| economy | 960×544 | 120×68 | 2 |
| balanced (default) | 1280×720 | 160×90 | 8 |
| detail | 1920×1080 | 240×135 | 16 |

`--profile economy`, `balanced` o `detail` sceglie un punto di partenza esplicito: non è una calibrazione automatica e non promette un frame rate. La densità non cambia durante il movimento. `--width`, `--height`, `--cols`, `--rows` e `--workers` possono essere sovrascritti indipendentemente dall'ordine degli argomenti. Sul PC con capacità stimata 16, i budget automatici sono 2/8/15.

`--backend auto|vulkan|dx12|metal|gl` seleziona il percorso grafico; un backend esplicito non viene sostituito silenziosamente. Auto rispetta anche `WGPU_BACKEND`. `--adapter-power auto|low|high` esprime una preferenza per la scheda, senza garantirne la disponibilità. `--fallback-adapter` richiede esplicitamente un adapter di fallback per diagnosi. I limiti/formati necessari vengono controllati prima della creazione delle risorse.

`--diagnostics --output evidence/my-machine` salva capacità effettive e configurazione in `runtime.json` dopo l'inizializzazione GPU. Un errore di compatibilità precedente viene stampato in console; non produce quel JSON. La diagnosi headless non verifica la superficie della finestra. Dettagli, risultati e piattaforme ancora da provare in [PORTABILITY.md](PORTABILITY.md).

## Prove riproducibili

```powershell
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
cargo run --locked --release --example cloth_study
.\target\release\ascii-engine-lab.exe --benchmark --frames 120 --output evidence\720p
.\target\release\ascii-engine-lab.exe --benchmark --frames 120 --width 1920 --height 1080 --cols 240 --rows 135 --output evidence\1080p
.\target\release\ascii-engine-lab.exe --headless --camera third --stage cloth --output evidence\capture
.\target\release\ascii-engine-lab.exe --sequence 90 --camera third --stage cloth --output evidence\sequence-third
.\target\release\ascii-engine-lab.exe --sequence 90 --camera third --orbit 2.4 --stage cloth --output evidence\sequence-cloth-back
.\target\release\ascii-engine-lab.exe --seconds 3
.\target\release\ascii-engine-lab.exe --workers 4
.\target\release\ascii-engine-lab.exe --profile economy --workers auto
.\target\release\ascii-engine-lab.exe --profile detail --backend dx12
.\target\release\ascii-engine-lab.exe --diagnostics --output evidence\my-machine
.\target\release\ascii-engine-lab.exe --benchmark-case --profile balanced --workers 2 --frames 120 --output evidence\my-machine-case
cargo run --locked --release --example snapshot_bench -- --paired
```

`--benchmark` visita quattro stage e tre camere; `--benchmark-case` visita soltanto camera/stage selezionati. Entrambi usano 12 frame di warm-up per caso, CSV di ogni frame misurato e JSON aggregato. Le PNG vengono dalla GPU reale anche in modalità senza finestra. `--sequence` salva fotogrammi a passo simulato di 1/30 s e piccola rotazione camera; non misura un frame rate di riproduzione. `--seconds` esegue uno smoke test a finestra con chiusura automatica. Per esportare un pacchetto confrontabile da un altro PC, conservare insieme runtime.json, benchmark.json e CSV, indicando anche modello CPU, sistema e condizioni di carico.

Output e griglia sono variabili diverse: 720p a 160×90 celle e 1080p a 240×135 mantengono 8×8 pixel per glifo, ma il secondo caso aumenta sia pixel sia query. Per isolare il costo GPU ripetere 1080p con `--cols 160 --rows 90`.

Tempi `submit_and_wait` includono driver, upload, lavoro GPU e sincronizzazione: **non sono timestamp GPU**. Frame offscreen sincronizzato non equivale a FPS/present della finestra. Il test statico controlla sia due composizioni da zero sia composizioni consecutive con storia. La metrica in movimento corrisponde soltanto a celle con identico ID nella stessa posizione schermo; riportare anche quante celle vengono abbinate. Non è una misura completa di sfarfallio né una riproiezione dei campioni.

## Approssimazioni e lavoro mancante

- Una query centrale per cella può perdere bordi, fili e dettagli piccoli. La quantizzazione dei glifi può produrre popping durante pan e zoom.
- Luce puntuale, ombre nette analitiche e termine ambientale artistico; niente illuminazione globale. Non tutti i materiali hanno già risposta speculare distinta.
- Acqua piana con normale perturbata e un solo rimbalzo; riflesso interrogato direttamente nello stesso mondo/tick, senza rifrazione, onde geometriche o simulazione fluidi.
- Figura da ellissoidi e tessuto analitico. Abito con onde locali e ancoraggi, non fisica del tessuto; tangenze e auto-intersezioni sono casi da migliorare.
- Grammatica visiva, densità e silhouette sono essenziali: non hanno ancora la qualità delle reference. Niente editor, gameplay, asset pipeline, networking o demo MMO.
- Il costo delle query del mondo è CPU; wgpu compone i caratteri. Trasferire query a compute richiede prima una misura che ne motivi il lavoro.

Il provino `cloth_study` usa una seconda implementazione di `Scene`: figura isolata, camera/luce ferme, output in `evidence/cloth-isolated/`. Serve a distinguere deformazione da movimento camera/luce; non cambia i benchmark del cortile.

Risultati e priorità successive in [REPORT.md](REPORT.md). Regia e prompt sono in `../ascii-game/docs/COORDINATION.md` e `CHAT_STARTERS.md`. Non creare altre chat senza richiesta umana; i subagenti ricevono file disgiunti dalla regia.

R1 in REPORT.md resta il riferimento storico. La prova CPU multicore, il significato delle misure e i prossimi passi sono in [PERFORMANCE.md](PERFORMANCE.md); i dati aggregati R2 sono in [summary.md](evidence/r2/scaling/summary.md). I test `r1_parity` compilano una copia immutabile dei sorgenti R1 in evidence/r2/baseline/src, esclusa dall'applicazione; conservarla insieme ai test.

R3 è descritta in [PORTABILITY.md](PORTABILITY.md). I test `snapshot_parity` e l'esempio `snapshot_bench` usano le copie R2 in `evidence/r3/baseline/src`. Conservare anche queste. Gli script `measure-scaling.ps1` e `summarize-scaling.py` documentano R2: non rieseguirli sul loro output storico con la build corrente; per un nuovo confronto usare una destinazione nuova e identificare la nuova revisione.

La build corrente R4 aggiunge `Scene::occluded` con default compatibile, specializzazione delle ombre del cortile e profiler per categorie di raggi. Risultati e comandi in [QUERY_PERFORMANCE.md](QUERY_PERFORMANCE.md). I test `query_parity` importano camera/compositore/mondo R3 da `evidence/r4/baseline/src`, da conservare immutabili. Non attribuire grandi accelerazioni a questa modifica: il beneficio misurato è modesto e variabile.

Per capire cosa manca al motore, come un gioco lo utilizzerà e come nasceranno gli strumenti, leggere [ENGINE_MILESTONES.md](../ascii-game/docs/ENGINE_MILESTONES.md). La prossima priorità è un provino visivo mirato; gameplay ed editor completo restano fuori dalla tranche.

## Fonti di API e dipendenze

[wgpu 26.0.1](https://docs.rs/wgpu/26.0.1/wgpu/), [winit 0.30.12](https://docs.rs/winit/0.30.12/winit/), [font8x8](https://docs.rs/font8x8/0.3.1/font8x8/). Le fonti documentano le API, non attestano qualità o prestazioni del laboratorio.
