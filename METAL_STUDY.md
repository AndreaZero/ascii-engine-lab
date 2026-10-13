# R9 — acciaio lavorato e ossidato

Provino circoscritto del 13 ottobre 2026. Una piastra/scudo bombata con bordo e otto rivetti condivide forma, camere e luce fra le finiture. Il metallo aggiunge al contratto locale una risposta da conduttore; il materiale costruisce direttamente colori e caratteri dalla propria superficie. Nessuna immagine di mondo convertita in ASCII.

## Avvio e confronto

```sh
cargo run --locked --release --example metal_study -- --window --variant forged --camera third
cargo run --locked --release --example metal_study -- --variant pair --camera all --frames 180 --out evidence/metal-study
cargo run --locked --release --example metal_study -- --benchmark --variant pair --camera all --workers 1 --frames 180 --out evidence/metal-cpu
```

[Avvia-metallo.cmd](Avvia-metallo.cmd) apre l'eseguibile esistente, compilando solo se manca. Dopo cambi ai sorgenti ricompilare con Cargo. Tasti: **V** lavorato/ossidato; **B** superficie uniforme/lavorata; **1/2/3** prima, terza e dall'alto; **O** camera/luce mobile; **Spazio** pausa, **R** ricomincia, **H** etichette, **Esc** esci. Ogni cambio finitura resetta la history.

`--variant uniform|forged|oxidized|pair|all`: uniform è la stessa geometria con normale geometrica, rugosità uniforme e metallo puro; forged aggiunge lavorazione e ossido di fucina residuo, oxidized più ossido. Uniform è un controllo semplificato, non un precedente risultato R8. `--path static|orbit|light|approach|showcase` separa i movimenti; showcase usa tre secondi di camera mobile e tre di luce mobile. Griglie economy 120×68, balanced 160×90, detail 240×135; output headless 1920×1080. La cadenza simulata di 30 Hz non è una misura FPS.

## Implementato

- [Forma analitica](examples/support/metal_geometry.rs): unione di dieci ellissoidi, con intervalli quadratici e superficie esterna dell'unione. Corpo convesso, bordo più largo e sottile, rivetti. Coordinate locali solidali all'oggetto; materiale di corpo/bordo/rivetti distinto. Nessuna mesh o raymarch.
- Guardia conservativa sul volume esterno: scarta i raggi che non possono raggiungere la piastra prima delle dieci intersezioni. Slab in f64, padding e tangenti inclusivi; raggio e matematica delle quadriche restano invariati. Il confronto con la query senza guardia include origini interne, raggi radenti, quasi paralleli e d'ombra. Sorgenti, eseguibile e prime misure precedenti la guardia sono conservati localmente.
- [Materiale locale](examples/support/metal_surface.rs): impressioni di martello a supporto finito, sette graffi rastremati, usura e chiazze di ossido persistenti. Normali decorative e dettaglio filtrati rispetto all'impronta della cella. Famiglie di glifi e chiavi di history usano la stessa classificazione locale.
- Estensione `MetallicSample { f0: Vec3, metalness: f32 }` e `Scene::metallic_sample`, opzionale. `f0` è l'endpoint conduttore RGB lineare, non il valore già miscelato. Roughness, normale, pigmento dell'ossido e occlusione rimangono in `MaterialSample`. None o metalness zero conserva l'aritmetica dielettrica R8.
- Nel ramo metallico il renderer miscela F0 dielettrico/conduttore, applica Schlick RGB e riduce diffusione diretta **e ambientale** per `1-metalness`. La normale geometrica continua a guidare visibilità e offset d'ombra. Shader, atlas e campionamento geometrico restano invariati.

La parametrizzazione segue il principio metalness descritto da [Filament](https://google.github.io/filament/main/filament.html#materialsystem/parameterization/remapping). È un'approssimazione RGB, diversa dal Fresnel complesso con eta/k descritto in [PBRT](https://www.pbr-book.org/4ed/Reflection_Models/Conductor_BRDF). Queste fonti motivano il modello; non validano il nostro risultato visivo o le prestazioni.

## Approssimazioni e parti mancanti

I valori dell'acciaio e dell'ossido sono artistici, non misure di una lega. Le finiture mescolano copertura metallica e dielettrica; non simulano chimica della corrosione o trasporto in un rivestimento stratificato. La martellatura cambia la normale decorativa, non la sagoma; la forma è regolare e solida, non una lamiera cava.

La risposta è isotropica con una sola luce puntuale e occlusione analitica. Nessuna riflessione d'ambiente/IBL, luce ad area, anisotropia o illuminazione indiretta. Il metallo puro in ombra può essere nero perché non riceve un contributo ambientale speculare: non viene schiarito con diffusione artificiale. Segni direzionali non equivalgono a un modello anisotropico.

Il filtro della rugosità e il limite di colore già esistenti possono allargare o saturare il riflesso. Il contatore `channel_limit_cells` misura celle della piastra con almeno un canale finale pari a 1; non ricostruisce la radiance tagliata prima del limite. La diagnostica di copertura riusa l'accumulatore della pietra con object ID 800, escludendo il pavimento: copertura dell'atlas e proxy lineari non sono luminanza percettiva o punteggi di realismo.

## Verifica e risultati

Verifica d'integrità PASS, budget prestazionale **non rispettato uniformemente**. Baseline R8 congelata da `395fd27` prima di modificare il renderer. [Sintesi pubblica](evidence/r9/summary.md); dati grezzi e manifest in `evidence/r9/`, esclusi da Git.

| Prova | Risultato |
| --- | --- |
| Test e build | Suite workspace/all-targets: 140 esecuzioni PASS. Dopo la guardia geometrica: 16 test mirati PASS. 156 esecuzioni complessive comprendono ripetizioni; inventario finale equivalente 141, non 141 comportamenti indipendenti. fmt, clippy workspace e build release PASS; warning ambientali della cache hardlink. |
| Preservazione R8 | 7/7 hash conservati; 87 PNG della pietra identiche alle registrazioni R8, incluse catture in movimento. Nessun cambio al runtime Godot. |
| Backend | 18 coppie PNG del metallo e 30 della pietra identiche fra Vulkan e DX12. Finestra Vulkan esercitata con successo. |
| Matrice | 72 casi, 10.800 frame: tre griglie, tre viste, camera/luce/distanza e statico. Finiture con geometria e percorso identici, colori intenzionalmente diversi. Nessun taglio o scomparsa dell'oggetto. |
| Guardia geometrica | 18 PNG complete identiche alla preview e 1.080 hash per-frame RGB/ID identici; hash RGB/ID esclude il codice glifo. Metriche di superficie e confronti CPU uguali. Test contro intersezioni senza guardia superato. |
| Filmato | 1.080 PNG wgpu, affiancate in 18 s / 2560×1080 / 30 Hz simulati / 540 frame. Margini ritagliati verificati, etichette, nessun ridimensionamento o correzione cromatica; decodifica completa PASS. |

Host Windows x64, Ryzen 7 3800X identificato nelle tranche precedenti, Radeon RX 6700 XT rilevata nei run, driver Vulkan 26.8.1; anche DX12 verificato. Hardware di prova, non requisiti minimi. 12 warmup esclusi. CPU-only: tre ripetizioni a 240×135, 180 frame per caso, ordine worker 1/8, 8/1, 1/8. Diagnostica, GPU e I/O fuori dal timer `Composer::compose`; audit CPU solo al primo/ultimo frame, audit GPU su tutti i frame. Build, benchmark e codifica serializzati dalla regia, carico complessivo del PC non isolato.

| Finitura / vista | P50 seriale | P50 con 8 worker | Peggior p95 con 8 worker |
| --- | ---: | ---: | ---: |
| Lavorato / prima | 17,34 ms | 4,57 ms | 9,66 ms |
| Lavorato / terza | 18,12 ms | 5,78 ms | 11,92 ms |
| Lavorato / alto | 23,68 ms | 5,20 ms | 11,56 ms |
| Ossidato / prima | 17,15 ms | 4,40 ms | 12,33 ms |
| Ossidato / terza | 18,81 ms | 6,35 ms | 18,51 ms |
| Ossidato / alto | 24,22 ms | 4,95 ms | 17,39 ms |

P50 = mediana dei tre p50, p95 = massimo fra i tre run. **2/18 run CPU a 8 worker superano il budget esplorativo di 16,7 ms; nella matrice sono 19/72, con peggior p95 28,66 ms.** Anche 5/6 registrazioni complete superano il budget, fino a p95 26,53 ms. I frame singoli hanno code maggiori dei percentili. Non sono FPS, non costituiscono una garanzia di fluidità e nessun run sfavorevole è stato rimosso.

Prima della guardia AABB, due run a 8 worker davano p50 mediani 11,05–14,63 ms; dopo, tre run danno 4,40–6,35 ms. È una riduzione osservata, con immagini preservate, ma le fasi non erano intercalate e l'attività dell'host non è stata isolata: non attribuiamo l'intero scarto alla sola patch o lo trasformiamo in un moltiplicatore FPS. Anche la pietra invariata mostrava forte variabilità nella sessione; nessuna causa unica dimostrata.

Working set: CPU-only 17,31–18,07 MiB, matrice 248,16–258,74 MiB, registrazione 274,05 MiB, runtime/statici 215,93–324,57 MiB. Memoria del processo/driver, non VRAM. Submission+attesa p50 nel filmato 1,07–1,26 ms, non GPU time.

## Resa visiva e stabilità

Metallo e ossido sono distinguibili dalla pietra e fra loro: bordo lucido, risposta localizzata alla luce, zone brune più opache. Alcune successioni di `u/n/parentesi` sembrano ancora una trama intrecciata; bordo destro e parte inferiore perdono continuità nel nero, con rivetti che possono apparire isolati. La forma resta regolare e la resa stilizzata. Questi limiti restano visibili, senza interventi sull'immagine registrata.

Nel filmato, le celle della piastra con almeno un canale al limite sono mediamente 0,45–1,57% fra le sei sequenze; massimo per singolo frame 2,70%. La quota di celle vuote sulla piastra è 17,89–22,05%. Sono conteggi tecnici della costruzione dell'immagine: i piccoli riflessi bianchi possono comunque perdere dettaglio, e il nero non è automaticamente un difetto di geometria.

Statico: zero cambi. Nella matrice, i cambi glifo sugli ID abbinati sono 0,15–0,39% con orbita, 0,19–0,58% con luce mobile e 0,12–0,34% con avvicinamento. Questi contatori includono il pavimento, escludono disocclusioni e comprendono cambi motivati; non certificano assenza di sfarfallio. La diagnostica di copertura del metallo esclude invece il pavimento. Il filmato serve al giudizio in movimento, distinto dai PASS tecnici.

Script: `scripts/verify-r9.py` esegue fasi preview, runtime, matrix, cpu e motion separatamente; `scripts/encode-r9.py` affianca le catture reali wgpu; `scripts/finalize-r9.py` verifica completezza, provenienza, parità e misure senza scambiare un superamento del budget per un errore d'integrità. Costruire gli esempi release prima di usarli. Non eseguire build/test, benchmark ed encoder contemporaneamente. Test Rust disponibili da clone; il finalizer della campagna completa richiede archivi R8, preview pre-guardia e relativi eseguibili/log locali, esclusi da Git. Non è un comando di test autosufficiente su clone pulito.

Filmato locale: `evidence/r9/video/materials/metallo-tre-visuali.mp4`. Prossimo limite da affrontare con una prova propria: luce e riflessi d'ambiente, valutati in una scena comune con pietra, metallo e tessuto. Nessuna nuova implementazione di queste capacità è inclusa in R9.

Lo stato da usare nel sito/docs è [PROJECT_STATUS.md](PROJECT_STATUS.md). Il laboratorio non è ancora un motore di produzione o un editor completo.
