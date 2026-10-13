# R2 — misure raccolte

Median of per-run percentiles; min/max are repeat spread, not confidence intervals.

Stage completo Cloth. Millisecondi; tre visuali dello stesso mondo. Ogni valore è la mediana dei percentili di ciascuna ripetizione, non il percentile di campioni uniti.

| Output | Camera | Variante | Rip. | CPU p50 | CPU p95 | Intervallo p95 fra ripetizioni | Frame sync p95 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1080p | first | r1 | 3 | 28.47 | 34.96 | 32.85–75.33 | 36.11 |
| 1080p | first | w1 | 3 | 26.92 | 32.02 | 27.73–35.90 | 32.89 |
| 1080p | first | w2 | 3 | 17.29 | 19.46 | 16.90–19.72 | 20.51 |
| 1080p | first | w4 | 3 | 9.43 | 10.30 | 10.29–11.61 | 11.63 |
| 1080p | first | w8 | 3 | 5.54 | 6.19 | 5.82–6.50 | 7.82 |
| 1080p | first | w16 | 3 | 4.42 | 5.67 | 5.50–9.18 | 7.29 |
| 1080p | overhead | r1 | 3 | 26.51 | 30.49 | 28.65–33.06 | 31.71 |
| 1080p | overhead | w1 | 3 | 26.40 | 31.52 | 27.01–35.83 | 32.82 |
| 1080p | overhead | w2 | 3 | 17.84 | 19.89 | 19.72–19.90 | 20.80 |
| 1080p | overhead | w4 | 3 | 9.06 | 9.80 | 9.12–12.01 | 10.62 |
| 1080p | overhead | w8 | 3 | 5.45 | 5.87 | 5.85–18.41 | 8.03 |
| 1080p | overhead | w16 | 3 | 4.45 | 7.04 | 5.33–9.59 | 8.11 |
| 1080p | third | r1 | 3 | 21.74 | 26.08 | 24.75–37.53 | 27.80 |
| 1080p | third | w1 | 3 | 22.38 | 27.15 | 22.80–36.08 | 28.65 |
| 1080p | third | w2 | 3 | 14.26 | 16.09 | 12.74–16.59 | 17.30 |
| 1080p | third | w4 | 3 | 7.85 | 8.63 | 7.68–8.89 | 10.56 |
| 1080p | third | w8 | 3 | 4.82 | 5.34 | 5.20–9.87 | 7.35 |
| 1080p | third | w16 | 3 | 3.78 | 4.60 | 4.40–15.52 | 6.43 |
| 720p | first | r1 | 3 | 12.62 | 16.50 | 15.02–17.50 | 17.34 |
| 720p | first | w1 | 3 | 12.42 | 16.12 | 15.93–16.41 | 17.10 |
| 720p | first | w2 | 3 | 8.08 | 9.70 | 7.68–13.72 | 11.27 |
| 720p | first | w4 | 3 | 3.94 | 4.52 | 4.49–5.50 | 6.15 |
| 720p | first | w8 | 3 | 3.84 | 8.01 | 3.92–8.18 | 8.90 |
| 720p | first | w16 | 3 | 2.46 | 3.14 | 2.65–4.81 | 5.15 |
| 720p | overhead | r1 | 3 | 13.54 | 17.12 | 13.28–20.85 | 18.43 |
| 720p | overhead | w1 | 3 | 11.66 | 14.78 | 13.40–14.85 | 15.64 |
| 720p | overhead | w2 | 3 | 7.78 | 9.36 | 7.93–10.27 | 10.75 |
| 720p | overhead | w4 | 3 | 4.25 | 4.56 | 4.55–10.90 | 5.92 |
| 720p | overhead | w8 | 3 | 2.85 | 4.43 | 2.93–4.93 | 6.61 |
| 720p | overhead | w16 | 3 | 2.25 | 3.17 | 2.53–3.37 | 4.64 |
| 720p | third | r1 | 3 | 10.17 | 13.13 | 10.90–18.10 | 14.30 |
| 720p | third | w1 | 3 | 10.09 | 12.77 | 11.77–13.35 | 13.50 |
| 720p | third | w2 | 3 | 5.08 | 6.40 | 5.64–7.44 | 7.40 |
| 720p | third | w4 | 3 | 3.61 | 4.36 | 4.12–4.58 | 6.08 |
| 720p | third | w8 | 3 | 2.58 | 3.93 | 3.47–3.99 | 5.96 |
| 720p | third | w16 | 3 | 1.89 | 2.61 | 2.43–5.54 | 4.73 |

Confronti catture GPU con R1: 360; diverse: 0. Test statici falliti: 0.

Percentuale CPU dell’intero processo sul tempo logico disponibile (16 processori logici); non saturazione dei core fisici o misura del solo compositore.

| Variante | Output | CPU processo mediana |
| --- | --- | --- |
| r1 | 1080p | 5.6% |
| r1 | 720p | 5.6% |
| w1 | 1080p | 5.8% |
| w1 | 720p | 5.7% |
| w2 | 1080p | 11.0% |
| w2 | 720p | 10.5% |
| w4 | 1080p | 18.4% |
| w4 | 720p | 17.3% |
| w8 | 1080p | 29.2% |
| w8 | 720p | 22.6% |
| w16 | 1080p | 38.3% |
| w16 | 720p | 33.3% |

Limiti: No GPU timestamps or window FPS; Worker count is not occupied physical cores; Whole-process CPU percent includes startup, captures, warm-up, driver and waits; Local desktop, background activity/frequency/temperature not controlled; R1 measured before R2; worker order reversed in repetition 2, not a randomized controlled study.
