# Prestazioni e uso CPU — R2, 13 ottobre 2026

Report storico R2. La build corrente, i nuovi profili e il confronto snapshot sono descritti in [PORTABILITY.md](PORTABILITY.md); le impostazioni predefinite riportate qui identificano R2 e non sostituiscono quelle R3.

L'obiettivo è aumentare la qualità sostenibile del motore ASCII, mantenendo costi e regolarità misurabili. Questa prova conserva contenuto e qualità R1: nessuna riduzione della griglia, dei raggi, dei riflessi o del solver del tessuto.

## Cosa usava R1

Hardware locale: AMD Ryzen 7 3800X 8-Core, 16 processori logici disponibili; AMD Radeon RX 6700 XT, Vulkan. La composizione CPU era un ciclo seriale: camera, intersezioni, luce, ombre, acqua e selezione dei glifi su un thread chiamante. Il driver grafico può impiegare altri thread: un ciclo seriale non significa un processo composto da un solo thread.

La GPU disegna già i glifi, ma non esegue ancora le query del mondo. R1 misurava durate, non percentuali di saturazione CPU/GPU. `codegen-units=1` è un'opzione del compilatore, non limita i core a runtime. I 16 thread logici condividono le risorse degli 8 core fisici; un worker non è fissato a un core e il loro numero non prova occupazione o efficienza.

## Cambiamento R2

Pool Rayon privato e persistente per compositore, conteggio esplicito; `--workers 1` conserva un percorso seriale senza pool. Celle indipendenti leggono la stessa scena immutabile; storia e statistiche mantengono ordine deterministico. Camera e valori comuni di luce vengono preparati una volta per fotogramma. Non vengono creati thread nuovi per ogni cella o fotogramma.

Il contratto del compositore richiede `Scene + Sync`: le implementazioni devono consentire letture concorrenti e fornire query deterministiche per lo stesso stato/tick. Nessuna mutazione di simulazione dentro le query. La presentazione wgpu resta sul percorso dell'applicazione.

## Risultato verificato

20 test PASS (16 unitari e 4 nella suite con riferimento R1), fmt/clippy all-targets PASS, build release e smoke a finestra completati (295 redraw in circa 3 secondi, non una misura display FPS). Tutte le 360 PNG GPU R2 sono byte-identiche alle catture R1 corrispondenti. Zero fallimenti statici nei 432 casi misurati; 51.840 frame cronometrati, oltre a warm-up e pan. La parità CPU con R1 copre anche animazione, camera in movimento, storia e reset.

Stage Cloth completo, 1080p / 240×135 caratteri, terza persona. Numeri sotto: mediana dei p50/p95 delle tre ripetizioni.

| Variante | CPU p50 ms | CPU p95 ms | Frame offscreen sync p95 ms |
| --- | --- | --- | --- |
| R1 originale, seriale | 21,74 | 26,08 | 27,80 |
| R2, 1 worker | 22,38 | 27,15 | 28,65 |
| R2, 2 worker | 14,26 | 16,09 | 17,30 |
| R2, 4 worker | 7,85 | 8,63 | 10,56 |
| R2, 8 worker | 4,82 | 5,34 | 7,35 |
| R2, 16 worker | 3,78 | 4,60 | 6,43 |

In questo caso 8 worker riducono la durata mediana CPU di circa 4,6 volte rispetto al nuovo seriale, 16 di circa 5,9. Da 8 a 16 si guadagna circa il 22% di durata CPU, non un raddoppio. Il nuovo seriale non mostra un miglioramento generale rispetto a R1: la preparazione e il nuovo scratch non vanno pubblicizzati come guadagno isolato; il vantaggio dimostrato è soprattutto il parallelismo.

Tutte le visuali e gli stage, con min/max fra ripetizioni, sono in [summary.md](evidence/r2/scaling/summary.md) e JSON/CSV adiacenti. A 1080p lo stage completo con 8 worker ha p50 CPU 4,82–5,54 ms nelle tre camere. Esistono spike: nella vista dall'alto con 8 worker una ripetizione ha p95 CPU 18,41 ms; con 16 worker in terza persona una ha p95 15,52 ms. Non è provato che una configurazione sia universalmente più stabile; niente garanzia di 60 FPS o scaling lineare.

Il processo completo a 1080p usa in media (mediana fra ripetizioni) circa 5,6% del tempo CPU logico disponibile in R1, 29,2% con 8 worker e 38,3% con 16. Comprende le attese e le altre fasi descritte sotto: non significa che il resto sia una percentuale equivalente di potenza fisica inutilizzata. L'utilizzo GPU percentuale e i timestamp GPU restano non misurati.

Default dell'app: min(8, processori logici disponibili), un budget provvisorio di worker, non core riservati né una scelta ottima dimostrata per qualsiasi gioco. `--workers 16` è disponibile; andrà rivalutato con contenuti e altri sistemi del gioco. `Composer::default()` nella libreria resta seriale, la parallelizzazione è esplicita con `with_threads`. Eseguibile del launcher aggiornato; Godot invariato.

## Protocollo

Sorgenti ed eseguibile R1 preservati in `evidence/r2/baseline/`. Script `scripts/measure-scaling.ps1`: tre ripetizioni, ordine worker invertito nella seconda, 1/2/4/8/16 worker, tutte le 3 camere e 4 stage, 120 frame + 12 warm-up per caso. Due configurazioni: output 1280×720 con 160×90 celle e 1920×1080 con 240×135. Tempi a simulazione deterministica dt=1/60 s, seed 0, orbit 0; una sola esecuzione alla volta. Baseline R1 ripetuta sullo stesso computer, senza compilazioni concorrenti.

La baseline binaria locale non è destinata al versionamento. Se manca, ricompilarla dai sorgenti preservati con `cargo build --locked --release --manifest-path evidence/r2/baseline/Cargo.toml --target-dir target/r1` e copiare l'eseguibile risultante in `evidence/r2/baseline/ascii-engine-lab-r1.exe`; il suo nuovo hash andrà registrato nelle prove. Nella consegna attuale hash di tutti i sorgenti baseline ed eseguibile coincidono con il manifest R1 originale.

CPU generation è durata della composizione sul percorso critico, non la somma del lavoro di tutti i thread. Submit/wait include driver, upload, GPU e sincronizzazione; non è un timestamp GPU. Offscreen sincronizzato non misura presentazione o FPS della finestra. p95 è la durata entro cui rientra circa il 95% dei campioni misurati: descrive i fotogrammi lenti meglio della sola media.

`process.json` raccoglie anche CPU time del processo e durata dell'esecuzione intera (incluse inizializzazione, PNG, warm-up, driver e attese), se il sistema espone il contatore. Il loro rapporto è il numero medio equivalente di processori logici occupati; diviso per 16 è una percentuale del tempo CPU logico disponibile, **non** saturazione dei core fisici né un contatore GPU. Non va confuso con il tempo per frame o confrontato come misura isolata del compositore.

Controlli: parità byte per byte con il compositore/camera R1 su 216 combinazioni di camera/stage/worker/frame, incluse storia/reset/pan; ulteriori confronti seriale/parallelo; catture GPU identiche a scena congelata. Non dimostrano da soli assenza di tutti gli artefatti percettivi.

## Passi successivi guidati dai risultati

1. Profilare query e iterazioni; preparare posa/luce e coefficienti del mondo una volta per tick, senza alterare la superficie del tessuto.
2. Ridurre intersezioni inutili con gruppi spaziali e query d'ombra con distanza massima. Una BVH generale va giustificata da scene più grandi; può costare più di quanto risparmia nella fixture attuale.
3. Confrontare SIMD solo in kernel individuati dal profilo, preservando una versione scalare e verificando casi radenti. Cambiare tipi vettoriali non dimostra automaticamente un vantaggio.
4. Provino GPU compute circoscritto, se motivato dal costo residuo: genera direttamente GlyphCell da dati semantici, conserva le celle sulla GPU, confronta errori numerici e timestamp validi. Nessun framebuffer RGB da convertire e nessuna mesh del mondo.
5. Impiegare il margine per silhouette, tessuto, materiali e campionamento delle celle ambigue. Misurare continuità nel movimento e tempi lenti su contenuti crescenti, non soltanto velocità di una scena piccola.

Puntare a un motore avanzato richiede questi confronti; non costituisce una dichiarazione di stato dell'arte raggiunto. Non serve ancora un editor o una riscrittura universale.

## Fonti primarie

[Rayon: pool e conteggio dei worker](https://docs.rs/rayon/latest/rayon/struct.ThreadPoolBuilder.html), [iteratori paralleli della versione 1.11](https://docs.rs/rayon/1.11.0/rayon/iter/trait.ParallelIterator.html), [PBRT: accelerazione delle intersezioni](https://www.pbr-book.org/4ed/Primitives_and_Intersection_Acceleration/Aggregates), [glam](https://docs.rs/glam/latest/glam/), [wgpu: compute](https://docs.rs/wgpu/latest/wgpu/struct.ComputePipeline.html). Le API attuali vanno sempre confrontate con le dipendenze fissate dal progetto. Queste fonti supportano i metodi, non attestano le prestazioni locali.
