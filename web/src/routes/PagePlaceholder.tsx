/** 六页共用的脚手架占位组件：只负责标题、说明与承接批次的标注。
 *  P5-5 起逐页替换为真实页面，占位组件随之退役。 */

interface PagePlaceholderProps {
  title: string;
  description: string;
  /** 负责数据接入的批次号（ROADMAP §6.3）。 */
  batch: string;
}

export function PagePlaceholder({ title, description, batch }: PagePlaceholderProps) {
  return (
    <section className="rounded-surface border border-base-300 bg-base-100">
      <div className="p-6">
        <h1 className="text-xl font-semibold">{title}</h1>
        <p className="mt-2 text-sm text-base-content/70">{description}</p>
        <div className="mt-4">
          <span className="badge badge-outline">数据接入批次：{batch}</span>
        </div>
      </div>
    </section>
  );
}
