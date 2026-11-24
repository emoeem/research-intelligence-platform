import CodeMirror, { type ReactCodeMirrorProps } from "@uiw/react-codemirror";
import { json } from "@codemirror/lang-json";
import { EditorView } from "@codemirror/view";
import { useMemo } from "react";
import { useUiText } from "../../i18n";

export function MessageEditor({
  value,
  onChange,
  language = "text",
  readOnly = false,
  label = "消息编辑器",
  minHeight = "240px",
  height,
  theme,
  className,
  testId,
  placeholder,
  basicSetup,
  extensions = [],
  onSave,
}: {
  value: string;
  onChange: (value: string) => void;
  language?: "text" | "json";
  readOnly?: boolean;
  label?: string;
  minHeight?: string;
  height?: string;
  theme?: ReactCodeMirrorProps["theme"];
  className?: string;
  testId?: string;
  placeholder?: string;
  basicSetup?: ReactCodeMirrorProps["basicSetup"];
  extensions?: ReactCodeMirrorProps["extensions"];
  onSave?: () => void;
}) {
  const ui = useUiText();
  const configured = useMemo(
    () => [...(language === "json" ? [json()] : []), EditorView.lineWrapping, ...extensions],
    [extensions, language],
  );
  return (
    <div
      className={`message-editor ${className ?? ""}`}
      data-testid={testId}
      aria-label={ui(label)}
      onKeyDown={(event) => {
        if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s" && onSave) {
          event.preventDefault();
          onSave();
        }
      }}
    >
      <CodeMirror
        aria-label={ui(label)}
        value={value}
        height={height ?? minHeight}
        minHeight={minHeight}
        theme={theme}
        placeholder={placeholder}
        extensions={configured}
        readOnly={readOnly}
        basicSetup={
          basicSetup ?? {
            lineNumbers: true,
            foldGutter: true,
            highlightActiveLine: true,
            highlightSelectionMatches: true,
          }
        }
        onChange={onChange}
      />
    </div>
  );
}
