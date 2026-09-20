// 生成最小 2 页测试 PDF：tests/fixtures/sample.pdf
// 用法：node scripts/make-fixture.mjs <输出路径>
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';

const out = process.argv[2] ?? 'src-tauri/tests/fixtures/sample.pdf';

function content(text) {
  const s = `BT /F1 24 Tf 72 720 Td (${text}) Tj ET`;
  return `<< /Length ${s.length} >>\nstream\n${s}\nendstream`;
}

const objs = [
  '<< /Type /Catalog /Pages 2 0 R >>',
  '<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>',
  '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 7 0 R >> >> >>',
  content('Hello PDFe Page 1'),
  '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 6 0 R /Resources << /Font << /F1 7 0 R >> >> >>',
  content('Hello PDFe Page 2'),
  '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
];

let pdf = '%PDF-1.7\n';
const offsets = [0];
for (let i = 0; i < objs.length; i++) {
  offsets.push(pdf.length);
  pdf += `${i + 1} 0 obj\n${objs[i]}\nendobj\n`;
}
const xrefStart = pdf.length;
pdf += `xref\n0 ${objs.length + 1}\n`;
pdf += '0000000000 65535 f \n';
for (let i = 1; i <= objs.length; i++) {
  pdf += String(offsets[i]).padStart(10, '0') + ' 00000 n \n';
}
pdf += `trailer\n<< /Size ${objs.length + 1} /Root 1 0 R >>\nstartxref\n${xrefStart}\n%%EOF\n`;

mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, pdf, 'latin1');
console.log('wrote', out, pdf.length, 'bytes');
