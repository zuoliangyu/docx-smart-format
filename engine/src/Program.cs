using System.Text.Encodings.Web;
using System.Text.Json;
using DocxWeb;

var argsList = args.ToList();
if (argsList.Count == 0)
{
    PrintUsage();
    return 1;
}

var command = argsList[0].ToLowerInvariant();
var options = ParseOptions(argsList.Skip(1));

try
{
    switch (command)
    {
        case "analyze":
            return RunAnalyze(options);
        case "build":
            return RunPlan(options);
        // Legacy, undocumented: kept only so the golden harness can keep
        // gauging pre-rewrite behavior until R4 removes the old engine.
        case "apply":
            return RunApply(options);
        case "render":
            return RunRender(options);
        default:
            Console.Error.WriteLine($"不支持的命令: {command}");
            PrintUsage();
            return 2;
    }
}
catch (Exception ex)
{
    Console.Error.WriteLine(ex.Message);
    return 1;
}

static int RunAnalyze(Dictionary<string, string> options)
{
    var input = RequireOption(options, "input");
    var output = RequireOption(options, "output");
    var assets = options.TryGetValue("assets", out var assetDir) ? assetDir : null;

    var analysis = DocxAnalyzer.Analyze(input, assets);
    WriteJson(output, analysis);
    return 0;
}

static int RunApply(Dictionary<string, string> options)
{
    var source = RequireOption(options, "source");
    var decisionPath = RequireOption(options, "decision");
    var output = RequireOption(options, "output");
    var template = options.TryGetValue("template", out var templatePath) ? templatePath : null;
    var templatePreset = options.TryGetValue("template-preset", out var preset) ? preset : null;
    var universityName = options.TryGetValue("university-name", out var uni) ? uni : string.Empty;
    var thesisTitle = options.TryGetValue("thesis-title", out var title) ? title : string.Empty;
    var workDir = options.TryGetValue("workdir", out var wd) ? wd : Path.GetDirectoryName(Path.GetFullPath(output));
    Directory.CreateDirectory(workDir!);

    var sourceDocId = Path.GetFileNameWithoutExtension(source);
    var sourceAssets = Path.Combine(workDir!, "assets", sourceDocId);
    Directory.CreateDirectory(sourceAssets);

    var sourceReport = DocxAnalyzer.Analyze(source, sourceAssets);
    var sourceAnalysis = DocumentAnalysisFactory.BuildSourceAnalysis(sourceDocId, sourceReport);

    TemplateAnalysis templateAnalysis;
    string? templateDocPath = null;
    if (string.Equals(templatePreset, "builtin-undergraduate-thesis", StringComparison.OrdinalIgnoreCase))
    {
        templateAnalysis = BuiltinTemplateFactory.CreateUndergraduateThesisTemplate(
            "builtin-undergraduate-thesis",
            universityName,
            thesisTitle);
    }
    else if (!string.IsNullOrWhiteSpace(template))
    {
        templateDocPath = template;
        var templateDocId = Path.GetFileNameWithoutExtension(template);
        var templateAssets = Path.Combine(workDir!, "assets", templateDocId);
        Directory.CreateDirectory(templateAssets);
        var templateReport = DocxAnalyzer.Analyze(template, templateAssets);
        templateAnalysis = DocumentAnalysisFactory.BuildTemplateAnalysis(templateDocId, templateReport, template);
    }
    else
    {
        templateAnalysis = BuiltinTemplateFactory.CreateUndergraduateThesisTemplate(
            "builtin-undergraduate-thesis",
            universityName,
            thesisTitle);
    }

    var decision = ReadJson<FormatDecisionEnvelope>(decisionPath);
    var renderSpec = TemplateApplyEngine.BuildRenderSpec(templateAnalysis, sourceAnalysis, decision);

    if (ShouldNormalizeReferences(options, templatePreset))
        ReferenceNormalizer.Normalize(renderSpec);

    DocxRenderer.Render(renderSpec, output, templateDocPath);
    return 0;
}

static int RunRender(Dictionary<string, string> options)
{
    var specPath = RequireOption(options, "spec");
    var output = RequireOption(options, "output");
    var template = options.TryGetValue("template", out var templatePath) ? templatePath : null;
    var templatePreset = options.TryGetValue("template-preset", out var preset) ? preset : null;

    var renderSpec = ReadJson<RenderSpec>(specPath);

    if (ShouldNormalizeReferences(options, templatePreset))
        ReferenceNormalizer.Normalize(renderSpec);

    DocxRenderer.Render(renderSpec, output, template);
    return 0;
}

static int RunPlan(Dictionary<string, string> options)
{
    var planPath = RequireOption(options, "plan");
    var output = RequireOption(options, "output");
    var template = options.TryGetValue("template", out var templatePath) ? templatePath : null;
    var sourcePath = options.TryGetValue("source", out var src) ? src : null;

    AnalysisReport? source = null;
    if (!string.IsNullOrWhiteSpace(sourcePath))
    {
        var workDir = options.TryGetValue("workdir", out var wd) ? wd : Path.GetDirectoryName(Path.GetFullPath(output));
        Directory.CreateDirectory(workDir!);
        var sourceAssets = Path.Combine(workDir!, "assets", Path.GetFileNameWithoutExtension(sourcePath));
        Directory.CreateDirectory(sourceAssets);
        source = DocxAnalyzer.Analyze(sourcePath, sourceAssets);
    }

    var plan = ReadJson<FormatPlan>(planPath);
    var renderSpec = FormatPlanCompiler.Compile(plan, source);

    if (ShouldNormalizeReferences(options, null))
        ReferenceNormalizer.Normalize(renderSpec);

    DocxRenderer.Render(renderSpec, output, template);
    return 0;
}

static bool ShouldNormalizeReferences(Dictionary<string, string> options, string? templatePreset)
{
    if (options.TryGetValue("normalize-references", out var flag))
    {
        if (string.IsNullOrWhiteSpace(flag)) return true;
        if (bool.TryParse(flag, out var parsed)) return parsed;
        return !string.Equals(flag, "0", StringComparison.Ordinal)
            && !string.Equals(flag, "false", StringComparison.OrdinalIgnoreCase)
            && !string.Equals(flag, "off", StringComparison.OrdinalIgnoreCase);
    }

    return string.Equals(templatePreset, "builtin-undergraduate-thesis", StringComparison.OrdinalIgnoreCase);
}

static Dictionary<string, string> ParseOptions(IEnumerable<string> args)
{
    var result = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
    string? pendingKey = null;

    foreach (var arg in args)
    {
        if (arg.StartsWith("--", StringComparison.Ordinal))
        {
            pendingKey = arg[2..];
            if (!result.ContainsKey(pendingKey))
                result[pendingKey] = string.Empty;
            continue;
        }

        if (pendingKey == null)
            continue;

        result[pendingKey] = arg;
        pendingKey = null;
    }

    return result;
}

static string RequireOption(Dictionary<string, string> options, string key)
{
    if (options.TryGetValue(key, out var value) && !string.IsNullOrWhiteSpace(value))
        return value;

    throw new InvalidOperationException($"缺少参数 --{key}");
}

static T ReadJson<T>(string path)
{
    var json = File.ReadAllText(path, System.Text.Encoding.UTF8);
    return JsonSerializer.Deserialize<T>(json, JsonOptions())
        ?? throw new InvalidOperationException($"无法解析 JSON: {path}");
}

static void WriteJson(string path, object data)
{
    var json = JsonSerializer.Serialize(data, JsonOptions());
    File.WriteAllText(path, json, System.Text.Encoding.UTF8);
}

static JsonSerializerOptions JsonOptions()
{
    return new JsonSerializerOptions
    {
        WriteIndented = true,
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping
    };
}

static void PrintUsage()
{
    Console.Error.WriteLine("用法:");
    Console.Error.WriteLine("  analyze --input <docx> --output <analysis.json> [--assets <dir>]");
    Console.Error.WriteLine("  build --plan <format-plan.json> --output <result.docx> [--source <docx>] [--template <template.docx>] [--normalize-references]");
    Console.Error.WriteLine("");
    Console.Error.WriteLine("  build 有 --source 即重排现有文档，无 --source 即从零生成。");
    Console.Error.WriteLine("  --normalize-references 显式传 false/off/0 可关闭参考文献规范化。");
}
