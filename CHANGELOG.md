# Historial de cambios

## 0.3.0 — 2026-11-21

### Añadido

- Estudio centrado en Trim, Split y Normalize.
- Edición con milisegundos, preescucha en bucle y fades.
- Split por capítulos, silencios y marcas manuales, con revisión de segmentos y ajuste manual del autosplit.
- Inspección de pico/RMS en dBFS con ventanas de 20 ms y controles basados en la pausa medida.
- Forma de onda con zoom hasta 64×, desplazamiento y regla temporal adaptativa.
- Normalización de sonoridad en dos pasadas y procesamiento por lotes.
- Cola con dos exportaciones simultáneas, cancelación, reintentos e historial.
- Publicación multiplataforma con comprobación de instaladores y sumas SHA-256.

### Corregido

- Los capítulos cortos se conservan sin aplicarles el mínimo de duración de silencios.
- Cambiar entre archivos de igual duración restablece correctamente la selección.
- Exportaciones simultáneas nunca sobrescriben resultados anteriores.
- Las tareas canceladas limpian sus archivos temporales de exportación.

### Retirado

- Descarga de canciones, playlists remotas, cookies y dependencia directa de yt-dlp.

## Versiones anteriores

Las notas históricas están disponibles en [GitHub Releases](https://github.com/NaturalDevCR/MediaToolsPro/releases).
