import { mount, signal, Text, View } from "nsh";
import { memoryBackend, PdfExport, PdfExportError, type PdfSource } from "./src";

/** A demo on the memory backend only: the headless test run needs no platform behind it. */
const backend = memoryBackend();
const pdf = new PdfExport(backend);
const source: PdfSource = { kind: "text", title: "Demo", pages: [{ lines: ["nsh-pdf-export"] }] };

const message = signal("nothing yet");

function codeOf(error: unknown): string {
  return error instanceof PdfExportError ? error.code : "unknown error";
}

function App() {
  const create = () => {
    pdf.createPdf(source).then(
      (bytes) => message.set(`created ${bytes.byteLength} bytes`),
      (error: unknown) => message.set(codeOf(error)),
    );
  };
  return (
    <View class="flex-1 gap-3 p-4">
      <Text testID="title" class="text-foreground">nsh-pdf-export</Text>
      <Text testID="message" class="text-foreground">{message()}</Text>
      <Text testID="create" role="button" onPress={create} class="rounded-(--radius) bg-muted px-3 py-2">
        Create
      </Text>
    </View>
  );
}

mount(App);
