# R9 — metallo e allineamento della documentazione

13 ottobre 2026. [Report](../../METAL_STUDY.md) · [Stato comune per sito/docs](../../PROJECT_STATUS.md).

- Piastra convessa analitica, bordo e otto rivetti; lavorato/ossidato con la stessa forma e luce. Segni di martello, graffi, usura e ossido locali filtrati. Resa ancora stilizzata, non asset definitivo.
- MetallicSample opzionale: endpoint F0 RGB e metalness, riduzione della diffusione diretta/ambientale, percorso None/m=0 preservato. Niente IBL, anisotropia o fisica della corrosione. Shader/atlas invariati.
- 140 esecuzioni nella suite workspace iniziale e 16 nel controllo mirato finale, tutte PASS. 156 esecuzioni includono ripetizioni; inventario equivalente finale 141. fmt/clippy/release PASS.
- 7/7 hash baseline R8 preservati, 87 PNG pietra identiche R8; 18 coppie metallo e 30 pietra identiche Vulkan/DX12; finestra PASS. Godot invariato.
- Matrice 72 casi / 10.800 frame, tre griglie e tre viste; statico senza cambi, nessun taglio. Guardia AABB conservativa con test contro query completa; 18 PNG e 1.080 hash RGB/ID identici prima/dopo. Hash RGB/ID non include il codice glifo.
- Tre ripetizioni CPU-only 1/8 worker, 180 frame/caso, 240×135: p50 mediani finali a 8 worker 4,40–6,35 ms. Prima della guardia, in due run non intercalati, 11,05–14,63 ms. Host non isolato: non attribuire tutto lo scarto alla patch.
- **Budget 16,7 ms non uniforme:** superato da 2/18 run CPU a 8 worker, 19/72 casi matrice e 5/6 registrazioni complete. Peggiori p95 rispettivamente 18,51 / 28,66 / 26,53 ms. Tutte le misure conservate; nessuna garanzia FPS.
- Windows x64, Ryzen 7 3800X identificato in precedenza, RX 6700 XT rilevata, Vulkan 26.8.1/DX12. Working set CPU 17,31–18,07 MiB, matrice 248,16–258,74 MiB, filmato 274,05 MiB; non VRAM.
- 1.080 PNG wgpu producono un filmato di 18 s / 2560×1080 / 540 frame / 30 Hz simulati, con crop verificato ed etichette. Decodifica completa PASS; media e dati grezzi non pubblicati in Git.
- Glifi talvolta ripetitivi, bordi scuri discontinui e riflessi localmente saturi. Celle al limite medio 0,45–1,57% nelle sequenze; non una misura di realismo. Riflessi d'ambiente ancora mancanti.

PROJECT_STATUS.md è la fonte comune per la conversazione laterale del sito/docs; contatto diretto e sincronizzazione bidirezionale non disponibili agli strumenti. R9 verificata non significa motore pronto per produzione o gameplay completo.
