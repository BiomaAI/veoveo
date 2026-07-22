{{- define "sumo.image" -}}
{{- $root := index . 0 -}}
{{- $image := index . 1 -}}
{{- $registry := trimSuffix "/" $root.Values.global.veoveoRegistry -}}
{{- $repository := $image.repository -}}
{{- if $registry -}}{{- $repository = printf "%s/%s" $registry $repository -}}{{- end -}}
{{- $tag := default $image.tag $root.Values.global.veoveoTag -}}
{{- printf "%s:%s" $repository $tag -}}
{{- end }}
