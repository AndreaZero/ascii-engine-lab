# R4 — risultati delle query

Median of per-run percentiles; min/max repeat spread is not a confidence interval

CPU Cloth 240×135, 120 frame per variante/ripetizione dopo 12 warm-up; tre ripetizioni abbinate per frame. Nessun tempo GPU/FPS.

| Worker | Camera | Variante | CPU p50 ms | CPU p95 ms | Intervallo p95 ms |
| --- | --- | --- | --- | --- | --- |
| 1 | first | nearest_shadow | 28.62 | 33.44 | 32.55–50.70 |
| 1 | first | occlusion | 28.09 | 32.49 | 31.90–44.15 |
| 1 | overhead | nearest_shadow | 29.65 | 34.78 | 33.35–42.37 |
| 1 | overhead | occlusion | 28.31 | 32.93 | 31.65–39.94 |
| 1 | third | nearest_shadow | 26.06 | 30.34 | 27.98–31.34 |
| 1 | third | occlusion | 25.14 | 28.49 | 27.90–30.19 |
| 8 | first | nearest_shadow | 5.35 | 8.68 | 5.36–70.16 |
| 8 | first | occlusion | 5.24 | 7.44 | 5.28–64.94 |
| 8 | overhead | nearest_shadow | 5.53 | 6.53 | 5.54–6.78 |
| 8 | overhead | occlusion | 5.43 | 6.37 | 5.32–6.40 |
| 8 | third | nearest_shadow | 4.79 | 6.25 | 4.72–12.06 |
| 8 | third | occlusion | 4.80 | 6.15 | 4.55–12.61 |

Baseline R3: 13 hash preservati. Catture: 24/24 identiche R3. Conteggi/risultati del replay invariati.

I replay prima/dopo avvengono in sessioni distinte; non attribuire il loro intero scarto alla modifica. Il confronto CPU abbinato sopra isola la variante nello stesso eseguibile.
