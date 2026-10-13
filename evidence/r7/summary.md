# R7 — verifica del provino pietra

13 ottobre 2026. Report e comandi in [STONE_STUDY.md](../../STONE_STUDY.md). Codice e sintesi pubblici; PNG, filmati, log, eseguibili e manifest grezzi restano locali.

- Windows x64, Ryzen 7 3800X (identificato nelle tranche precedenti), RX 6700 XT rilevata per run, Vulkan 26.8.1 e DX12. Una macchina fisica.
- fmt/clippy/build release PASS; 114 esecuzioni test workspace incluse copie storiche, 14 del provino ripetute dopo la correzione dell'avvicinamento.
- 24 PNG cortile e 12 Material Room identiche a R6. 24 coppie di PNG pietra identiche fra Vulkan/DX12. 6/6 hash baseline R6 preservati, Godot invariato.
- 72 casi / 9.774 frame: 3 griglie × 4 percorsi × 3 viste × 2 materiali. Statico senza cambi; dry/wet stessa geometria/camera/luce; nessun taglio finale. Primo errore di framing conservato localmente e corretto nel percorso camera.
- 3 ripetizioni CPU separate, 180 frame per caso, 12 warmup esclusi, 240×135 celle, 1/8 worker in ordine alternato. Mediane dei p50: 17,04–23,78 ms seriale, 3,66–5,02 ms con 8 worker; peggior p95 a 8 worker 6,15 ms. Budget esplorativo 16,7 ms rispettato sull'host corrente. I passaggi della matrice includono un p95 balanced di 13,90 ms; non omesso.
- Working set CPU-only 17,52–18,56 MiB; matrice GPU 246,48–255,21 MiB; runtime statico/finestra 214,90–324,92 MiB. Include driver/host, non misura VRAM. Submission+attesa detail p50 0,89–0,98 ms, non GPU time.
- Stabilità sull'intera immagine: cambi glifo fra identità abbinate 0% statico, 0,49–1,52% orbita, 0,42–0,97% luce, 0,20–0,67% avvicinamento. Cambi motivati e pavimento inclusi, identità diverse escluse; non è una misura completa dello sfarfallio.
- 1.080 PNG reali wgpu → confronto affiancato 18 s, 2560×1080, 30 frame/s, 540 frame, decodifica completa PASS. Solo crop dei margini verificato per frame ed etichette. Cadenza simulata, non FPS misurati.

Limiti: materiale ancora stilizzato, facce planari, crepe molto nette; il bagnato può perdere dettaglio nelle zone scure. Bagnatura artistica a uno strato, nessuna simulazione d'acqua. Filtraggio del materiale non equivale ad antialiasing geometrico. Nessun confronto di superiorità con altri motori o certificazione multipiattaforma.
