import { lazy, Suspense, type ComponentProps } from "react";

// CodeMirror is a good part of the bundle and only needed on the config
// screens, so it loads when one of them first shows an editor.
const load = () => import("./ConfigEditor");
const Editor = lazy(() => load().then((m) => ({ default: m.ConfigEditor })));
const Text = lazy(() => load().then((m) => ({ default: m.ConfigText })));

/** Holds the editor's place while it loads, so the layout doesn't jump. */
function Placeholder() {
  return <div className="min-h-32 rounded-lg bg-surface-raised" aria-hidden />;
}

export function ConfigEditor(props: ComponentProps<typeof Editor>) {
  return (
    <Suspense fallback={<Placeholder />}>
      <Editor {...props} />
    </Suspense>
  );
}

export function ConfigText(props: ComponentProps<typeof Text>) {
  return (
    <Suspense fallback={<Placeholder />}>
      <Text {...props} />
    </Suspense>
  );
}
