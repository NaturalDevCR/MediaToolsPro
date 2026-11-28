import { readonly, ref, watch } from "vue";
import { studioEnglish } from "./studio-english.ts";

type Locale = "es" | "en";
type TranslationParams = Record<string, string | number>;

const localeKey = "mediatoolspro.locale";
const english = new Map<string, string>(Object.entries({
  ...studioEnglish,
  "Estudio": "Studio",
  "Actividad": "Activity",
  "Ajustes": "Settings",
  "Navegación principal": "Main navigation",
  "Idioma": "Language",
  "MENOS PASOS. MEJOR AUDIO.": "FEWER STEPS. BETTER AUDIO.",
  "Usar tema claro": "Switch to light theme",
  "Usar tema oscuro": "Switch to dark theme",
  "Estudio iniciado": "Studio started",
  "Estudio listo": "Studio ready",
  "1 tarea en cola": "1 task in queue",
  "{count} tareas en cola": "{count} tasks in queue",
  "CONFIGURACIÓN DEL ESTUDIO": "STUDIO CONFIGURATION",
  "Motor de procesamiento": "Processing engine",
  "Comprobando motor…": "Checking engine…",
  "Motor disponible": "Engine available",
  "Motor no disponible": "Engine unavailable",
  "Trim, Split y Normalize utilizan FFmpeg en tu equipo. El estudio reconoce tanto la instalación de MediaToolsPro como una instalación del sistema. El procesamiento de tus archivos se realiza localmente.": "Trim, Split, and Normalize use FFmpeg on your computer. The studio works with either the MediaToolsPro installation or a system installation. Your files are processed locally.",
  "Volver a comprobar": "Check again",
  "Instalando motor…": "Installing engine…",
  "Instalar FFmpeg": "Install FFmpeg",
  "La instalación necesita conexión a internet y puede tardar unos minutos.": "Installation requires an internet connection and may take a few minutes.",
  "FFmpeg no está disponible. Instálalo desde Ajustes.": "FFmpeg is unavailable. Install it from Settings.",
  "Instala FFmpeg con el gestor de paquetes de tu sistema y vuelve a comprobar.": "Install FFmpeg with your system's package manager, then check again.",
  "El paquete no contiene FFmpeg.": "The package does not contain FFmpeg.",
  "No se pudo ejecutar el motor descargado.": "Could not run the downloaded engine.",
  "Actividad del estudio": "Studio activity",
  "Todo": "All",
  "Sistema": "System",
  "Vídeo": "Video",
  "Aviso": "Warning",
  "Éxito": "Success",
  "Limpiar": "Clear",
  "No hay actividad para mostrar": "No activity to display",
  "En espera": "Waiting",
  "Procesando": "Processing",
  "Completado": "Completed",
  "Cancelado": "Cancelled",
  "Mostrar resultado en carpeta": "Show result in folder",
  "Reintentar": "Retry",
  "Cancelar tarea": "Cancel task",
  "Quitar del historial": "Remove from history",
  "No se pudo reproducir la preescucha A/B.": "Could not play the A/B preview.",
  "Pausar comparación": "Pause comparison",
  "Reproducir comparación": "Play comparison",
  "Detener comparación": "Stop comparison",
  "Detener reproducción": "Stop playback",
  "Reproducir": "Play",
  "Detener": "Stop",
  "Pausar segmento {index}": "Pause segment {index}",
  "Avance del segmento {index}": "Segment {index} progress",
  "Preparando la versión A…": "Preparing version A…",
  "Preparando la versión B…": "Preparing version B…",
  "Progreso de la preescucha A/B": "A/B preview progress",
  "Versión del audio": "Audio version",
  "B · Con fades": "B · With fades",
  "B · Sin cambios": "B · Unchanged",
  "Posición de la comparación": "Comparison position",
  "No se pudo cargar la versión original.": "Could not load the original version.",
  "No se pudo cargar la versión procesada.": "Could not load the processed version.",
  "La sesión anterior se interrumpió. Puedes reintentar la tarea.": "The previous session was interrupted. You can retry the task.",
  "No se pudo recuperar el historial de exportaciones.": "Could not recover export history.",
  "No se pudo guardar el historial.": "Could not save history.",
  "El motor de escritorio todavía no está listo.": "The desktop engine is not ready yet.",
  "No hay capítulos utilizables dentro de la selección. Prueba el modo por silencios o añade cortes manuales.": "There are no usable chapters in the selection. Try splitting by silence or add manual cuts.",
  "Introduce una duración de al menos 0.001 s por segmento.": "Enter a duration of at least 0.001 s per segment.",
  "La duración por segmento debe ser menor que la selección.": "The segment duration must be shorter than the selection.",
  "La división supera los 1000 segmentos. Aumenta la duración por segmento.": "This split exceeds 1,000 segments. Increase the segment duration.",
  "Duración inválida.": "Invalid duration.",
  "Tiempo inválido: {time}": "Invalid time: {time}",
  "El inicio y el final deben formar un intervalo válido dentro del archivo.": "The start and end must form a valid range within the file.",
  "No se encontró un nombre de exportación disponible.": "No available export name was found.",
  "El nombre de salida requiere {numero} y admite {archivo}. Evita los caracteres reservados.": "The output name requires {numero} and supports {archivo}. Avoid reserved characters.",
  "El archivo de origen no existe.": "The source file does not exist.",
  "La normalización encadenada requiere un método de división.": "Chained normalization requires a split method.",
  "La ventana de búsqueda de pausas debe estar entre 0 y 60 s.": "The pause search window must be between 0 and 60 s.",
  "Los parámetros de procesamiento están fuera de rango.": "The processing parameters are out of range.",
  "Identificador de preescucha inválido.": "Invalid preview ID.",
  "El segmento de preescucha debe estar dentro de la selección.": "The preview segment must be within the selection.",
  "No se pudo reservar un archivo temporal.": "Could not reserve a temporary file.",
  "No se pudo medir la sonoridad.": "Could not measure loudness.",
  "Medición incompleta.": "Measurement incomplete.",
  "No se puede normalizar audio vacío, silencioso o sin sonoridad medible.": "Empty or silent audio and audio with no measurable loudness cannot be normalized.",
  "No se pudo publicar la exportación sin sobrescribir archivos.": "Could not publish the export without overwriting files.",
  "No se pudo leer el análisis de audio.": "Could not read the audio analysis.",
  "No se pudo leer la forma de onda.": "Could not read the waveform.",
  "No se pudo medir el nivel del audio.": "Could not measure the audio level.",
  "Normalizando la selección": "Normalizing selection",
  "Midiendo sonoridad · segmento {index}/{count} · pasada 1/2": "Measuring loudness · segment {index}/{count} · pass 1/2",
  "Cancelado por el usuario": "Cancelled by user",
  "Procesamiento terminado": "Processing finished",
  "División terminada con {count} archivos": "Split finished with {count} files",
  "1 segmento preparado": "1 segment prepared",
  "{count} segmentos preparados": "{count} segments prepared",
  "Segmento {index}/{count} → {output}": "Segment {index}/{count} → {output}",
  "Procesamiento en cola para {path}": "Queued media processing for {path}",
  "Procesamiento cancelado para {path}": "Processing cancelled for {path}",
  "Error al procesar {detail}": "Processing failed for {detail}",
  "Procesando segmento {index}/{count} de {path}": "Rendering segment {index}/{count} for {path}",
  "Procesamiento terminado para {path} con 1 segmento": "Processing finished for {path} with 1 segment",
  "Procesamiento terminado para {path} con {count} segmentos": "Processing finished for {path} with {count} segments",
}));

function initialLocale(): Locale {
  try {
    const saved = typeof window !== "undefined" && window.localStorage.getItem(localeKey);
    if (saved === "es" || saved === "en") return saved;
  } catch {
    // The current session can still switch languages if storage is unavailable.
  }
  const language = typeof navigator === "undefined" ? "" : navigator.language;
  return language.toLowerCase().split("-")[0] === "es" ? "es" : "en";
}

const activeLocale = ref(initialLocale());
export const locale = readonly(activeLocale);

watch(activeLocale, (language) => {
  if (typeof document !== "undefined") document.documentElement.lang = language;
}, { immediate: true, flush: "sync" });

export function setLocale(language: Locale) {
  activeLocale.value = language;
  try {
    if (typeof window !== "undefined") window.localStorage.setItem(localeKey, language);
  } catch {
    // Keep the selected language for the rest of the current session.
  }
}

export function t(source: string, params?: TranslationParams): string {
  const message = locale.value === "en" ? english.get(source) ?? source : source;
  return params
    ? message.replace(/\{(\w+)\}/g, (placeholder, name: string) =>
        Object.prototype.hasOwnProperty.call(params, name)
          ? String(params[name])
          : placeholder,
      )
    : message;
}

export function tMediaMessage(message: string): string {
  switch (message) {
    case "Cancelled by user":
      return t("Cancelado por el usuario");
    case "Processing finished":
      return t("Procesamiento terminado");
  }

  const invalidTime = /^Tiempo inválido: ([\s\S]*)$/.exec(message);
  if (invalidTime) return t("Tiempo inválido: {time}", { time: invalidTime[1] });
  const prepared = /^Prepared (\d+) segment\(s\)$/.exec(message);
  if (prepared) {
    return prepared[1] === "1"
      ? t("1 segmento preparado")
      : t("{count} segmentos preparados", { count: prepared[1] });
  }
  const segment = /^Segment (\d+)\/(\d+) -> ([\s\S]+)$/.exec(message);
  if (segment) {
    return t("Segmento {index}/{count} → {output}", {
      index: segment[1], count: segment[2], output: segment[3],
    });
  }
  const loudness = /^Midiendo sonoridad · segmento (\d+)\/(\d+) · pasada 1\/2$/.exec(message);
  if (loudness) {
    return t("Midiendo sonoridad · segmento {index}/{count} · pasada 1/2", {
      index: loudness[1], count: loudness[2],
    });
  }
  const split = /^Split finished with (\d+) files$/.exec(message);
  if (split) return t("División terminada con {count} archivos", { count: split[1] });
  const queued = /^Queued media processing for ([\s\S]+)$/.exec(message);
  if (queued) return t("Procesamiento en cola para {path}", { path: queued[1] });
  const cancelled = /^Processing cancelled for ([\s\S]+)$/.exec(message);
  if (cancelled) return t("Procesamiento cancelado para {path}", { path: cancelled[1] });
  const failed = /^Processing failed for ([\s\S]+)$/.exec(message);
  if (failed) return t("Error al procesar {detail}", { detail: failed[1] });
  const rendering = /^Rendering segment (\d+)\/(\d+) for ([\s\S]+)$/.exec(message);
  if (rendering) {
    return t("Procesando segmento {index}/{count} de {path}", {
      index: rendering[1], count: rendering[2], path: rendering[3],
    });
  }
  const finished = /^Processing finished for ([\s\S]+) with (\d+) segment\(s\)$/.exec(message);
  if (finished) {
    return finished[2] === "1"
      ? t("Procesamiento terminado para {path} con 1 segmento", { path: finished[1] })
      : t("Procesamiento terminado para {path} con {count} segmentos", {
          path: finished[1], count: finished[2],
        });
  }
  return t(message);
}
