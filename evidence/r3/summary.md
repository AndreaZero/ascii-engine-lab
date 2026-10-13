# R3 — snapshot e compatibilità

Median of per-run percentiles; ranges show repeat spread, not confidence intervals

CPU only, 240×135 celle, Cloth, 120 frame misurati per ripetizione dopo 12 warm-up. Nessun tempo GPU o FPS finestra. Il riferimento R2 e lo snapshot sono ricompilati nello stesso eseguibile. Varianti alternate all'interno di ciascun frame, ordine invertito a ogni frame/ripetizione. Il primo esperimento a blocchi, più sensibile alle oscillazioni di carico, è conservato in cpu/.

| Worker | Camera | Variante | Rip. | CPU p50 ms | CPU p95 ms | Intervallo p95 ms |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | first | r2_reference | 3 | 35.56 | 38.68 | 34.16–120.27 |
| 1 | first | snapshot | 3 | 30.03 | 33.50 | 28.63–83.13 |
| 1 | overhead | r2_reference | 3 | 33.39 | 36.91 | 36.18–112.38 |
| 1 | overhead | snapshot | 3 | 28.08 | 32.10 | 31.77–101.93 |
| 1 | third | r2_reference | 3 | 29.49 | 33.53 | 31.38–43.42 |
| 1 | third | snapshot | 3 | 24.83 | 29.37 | 27.12–37.38 |
| 8 | first | r2_reference | 3 | 5.66 | 6.26 | 5.90–6.85 |
| 8 | first | snapshot | 3 | 5.07 | 6.02 | 5.48–6.03 |
| 8 | overhead | r2_reference | 3 | 5.37 | 6.23 | 5.80–7.39 |
| 8 | overhead | snapshot | 3 | 4.83 | 5.75 | 5.46–7.62 |
| 8 | third | r2_reference | 3 | 4.95 | 5.85 | 5.37–9.16 |
| 8 | third | snapshot | 3 | 4.53 | 5.16 | 4.85–7.87 |

Baseline R2: 13/13 hash preservati.
Catture GPU identiche byte per byte a R2/Vulkan: 24/24.

Queste prove usano una sola macchina fisica; pochi worker non emulano CPU meno potenti.
