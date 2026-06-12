{{- define "loadr.name" -}}
{{- .Chart.Name -}}
{{- end -}}

{{- define "loadr.fullname" -}}
{{- printf "%s-%s" .Release.Name .Chart.Name | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "loadr.image" -}}
{{- printf "%s:%s" .Values.image.repository (default .Chart.AppVersion .Values.image.tag) -}}
{{- end -}}

{{- define "loadr.labels" -}}
app.kubernetes.io/name: {{ include "loadr.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ default .Chart.AppVersion .Values.image.tag | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version }}
{{- end -}}

{{- define "loadr.uiSecretName" -}}
{{- if .Values.controller.ui.existingSecret -}}
{{- .Values.controller.ui.existingSecret -}}
{{- else -}}
{{- include "loadr.fullname" . }}-ui-auth
{{- end -}}
{{- end -}}
