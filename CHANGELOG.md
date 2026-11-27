# Historial de cambios

## 0.5.1 — 2026-11-27

- Windows ejecuta FFmpeg en segundo plano para evitar las ventanas de consola al importar audio.
- La forma de onda de grabaciones largas se carga en bloques y conserva las mediciones cada 20 ms.

## 0.5.0 — 2026-11-23

- Detección de pausas adaptada a los niveles del audio, con sensibilidad y análisis automático opcional.
- Zoom por segundos y salto a un punto de la forma de onda en grabaciones largas.
- Exportación AIFF, AAC y MOV y opciones de codificación según el formato.
- Interfaz en español e inglés con selector de idioma.

## 0.4.2 — 2026-11-23

- El instalador de macOS sella la app completa con una firma ad hoc.
- La publicación comprueba la firma de la app dentro de los DMG de Apple Silicon e Intel.

## 0.4.1 — 2026-11-23

- Nuevo icono de MediaToolsPro en el estudio y en los instaladores de escritorio.

## 0.4.0 — 2026-11-23

- División por duración fija con ajuste opcional a pausas cercanas.
- Fades por segmento en los cuatro métodos de Split y normalización antes o después de dividir.
- Preescucha A/B del audio original y procesado.
- Presets locales de Split y nombres de exportación con `{archivo}` y `{numero}`.

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
