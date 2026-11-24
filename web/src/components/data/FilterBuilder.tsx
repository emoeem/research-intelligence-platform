import { useId, useRef } from "react";
import { useUiText } from "../../i18n";

export type FilterFieldType = "text" | "number" | "select" | "date" | "boolean";
export type FilterOperator =
  | "contains"
  | "equals"
  | "notEquals"
  | "startsWith"
  | "gt"
  | "gte"
  | "lt"
  | "lte"
  | "in"
  | "isEmpty"
  | "isNotEmpty";
export interface FilterField {
  id: string;
  label: string;
  type: FilterFieldType;
  options?: readonly { value: string; label: string }[];
  operators?: readonly FilterOperator[];
}
export interface FilterRule {
  id: string;
  field: string;
  operator: FilterOperator;
  value: string;
}
const DEFAULT_OPERATORS: Record<FilterFieldType, readonly FilterOperator[]> = {
  text: ["contains", "equals", "notEquals", "startsWith", "isEmpty", "isNotEmpty"],
  number: ["equals", "notEquals", "gt", "gte", "lt", "lte", "isEmpty", "isNotEmpty"],
  select: ["equals", "notEquals", "in", "isEmpty", "isNotEmpty"],
  date: ["equals", "gt", "gte", "lt", "lte", "isEmpty", "isNotEmpty"],
  boolean: ["equals", "notEquals"],
};
const OPERATOR_LABELS: Record<FilterOperator, string> = {
  contains: "包含",
  equals: "等于",
  notEquals: "不等于",
  startsWith: "开头为",
  gt: "大于",
  gte: "大于等于",
  lt: "小于",
  lte: "小于等于",
  in: "属于",
  isEmpty: "为空",
  isNotEmpty: "不为空",
};
function defaultOperator(field: FilterField): FilterOperator {
  return (field.operators ?? DEFAULT_OPERATORS[field.type])[0] ?? "equals";
}

export function FilterBuilder({
  fields,
  rules,
  onChange,
  label = "高级筛选",
  addLabel = "添加条件",
  clearLabel = "清空",
  applyLabel = "条件已实时生效",
}: {
  fields: readonly FilterField[];
  rules: readonly FilterRule[];
  onChange: (rules: FilterRule[]) => void;
  label?: string;
  addLabel?: string;
  clearLabel?: string;
  applyLabel?: string;
}) {
  const id = useId();
  const sequence = useRef(0);
  const ui = useUiText();
  const addRule = () => {
    const field = fields[0];
    if (field)
      onChange([
        ...rules,
        {
          id: `${id}-${sequence.current++}-${field.id}`,
          field: field.id,
          operator: defaultOperator(field),
          value: "",
        },
      ]);
  };
  const updateRule = (ruleId: string, patch: Partial<FilterRule>) =>
    onChange(
      rules.map((rule) => {
        if (rule.id !== ruleId) return rule;
        const next = { ...rule, ...patch };
        if (patch.field) {
          const field = fields.find((item) => item.id === patch.field);
          if (field) {
            next.operator = defaultOperator(field);
            next.value = "";
          }
        }
        return next;
      }),
    );
  return (
    <fieldset className="filter-builder" aria-label={label}>
      <legend className="text-xs font-semibold text-muted">{label}</legend>
      <div className="flex flex-col gap-2">
        {rules.map((rule) => {
          const field = fields.find((item) => item.id === rule.field) ?? fields[0];
          if (!field) return null;
          const operators = field.operators ?? DEFAULT_OPERATORS[field.type];
          const noValue = rule.operator === "isEmpty" || rule.operator === "isNotEmpty";
          return (
            <div
              className="flex flex-wrap items-center gap-2"
              key={rule.id}
              data-testid="filter-rule"
            >
              <label className="sr-only" htmlFor={`${rule.id}-field`}>
                字段
              </label>
              <select
                id={`${rule.id}-field`}
                className="select select-bordered select-sm"
                value={field.id}
                onChange={(event) => updateRule(rule.id, { field: event.target.value })}
              >
                {fields.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.label}
                  </option>
                ))}
              </select>
              <label className="sr-only" htmlFor={`${rule.id}-operator`}>
                操作符
              </label>
              <select
                id={`${rule.id}-operator`}
                className="select select-bordered select-sm"
                value={rule.operator}
                onChange={(event) =>
                  updateRule(rule.id, { operator: event.target.value as FilterOperator, value: "" })
                }
              >
                {operators.map((operator) => (
                  <option key={operator} value={operator}>
                    {ui(OPERATOR_LABELS[operator])}
                  </option>
                ))}
              </select>
              {!noValue &&
                (field.type === "select" || field.type === "boolean" ? (
                  <select
                    aria-label={`${field.label}筛选值`}
                    className="select select-bordered select-sm"
                    value={rule.value}
                    onChange={(event) => updateRule(rule.id, { value: event.target.value })}
                  >
                    <option value="">{ui("选择值")}</option>
                    {(field.type === "boolean"
                      ? [
                          { value: "true", label: "是" },
                          { value: "false", label: "否" },
                        ]
                      : (field.options ?? [])
                    ).map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                ) : (
                  <input
                    aria-label={`${field.label}筛选值`}
                    className="input input-bordered input-sm min-w-32"
                    type={
                      field.type === "number" ? "number" : field.type === "date" ? "date" : "text"
                    }
                    value={rule.value}
                    onChange={(event) => updateRule(rule.id, { value: event.target.value })}
                  />
                ))}
              <button
                type="button"
                className="btn btn-ghost btn-xs"
                aria-label={`删除${field.label}条件`}
                onClick={() => onChange(rules.filter((item) => item.id !== rule.id))}
              >
                {ui("移除")}
              </button>
            </div>
          );
        })}
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            className="btn btn-outline btn-sm"
            disabled={fields.length === 0}
            onClick={addRule}
          >
            {addLabel}
          </button>
          {rules.length > 0 && (
            <button type="button" className="btn btn-ghost btn-sm" onClick={() => onChange([])}>
              {clearLabel}
            </button>
          )}
          <span className="text-xs text-muted" aria-live="polite">
            {applyLabel}
          </span>
        </div>
      </div>
    </fieldset>
  );
}
