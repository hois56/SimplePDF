# SimplePDF

Windows에서 PDF를 읽고, 문구를 직접 수정하고, Word·Excel·이미지로 변환하는 데스크톱 앱입니다. 문서는 PC에서 처리하며, PDF 보기와 편집에 계정이 필요하지 않습니다.

**[Windows x64 설치 파일 다운로드](https://github.com/hois56/SimplePDF/releases/download/v1.0.1/SimplePDF-Setup-1.0.1.exe)** · [포터블 ZIP](https://github.com/hois56/SimplePDF/releases/download/v1.0.1/SimplePDF-1.0.1-portable.zip)

## 사용하기

Windows 10 버전 2004 이상 또는 Windows 11에서 설치 파일을 실행하세요. PDF를 더블클릭해서 열려면 Windows 기본 앱 설정에서 `.pdf`에 SimplePDF를 지정하세요. 포터블 버전은 ZIP 전체를 풀고 `SimplePDF.exe`를 실행하세요.

- PDF 여러 개를 한 줄의 탭으로 열고, 책갈피와 문서 검색으로 이동할 수 있습니다. 페이지 맞춤과 두 페이지 보기를 지원합니다.
- 문서 위의 텍스트를 클릭해 그 자리에서 수정하고, 글꼴·크기·색상을 조정할 수 있습니다.
- PDF에 이미지를 추가하거나 기존 사진을 같은 자리에 교체할 수 있습니다. 이미지 편집 중에는 사진 위에 원본 픽셀 크기가 표시되며, 사진을 우클릭하면 PNG로 저장할 수 있습니다.
- PDF 페이지를 병합·추출·회전하거나 순서를 바꿀 수 있습니다.
- PDF를 Word 문서, Excel 표, PNG 또는 JPG로 내보낼 수 있습니다. Office 문서를 PDF로 변환할 때는 해당 Microsoft Office 앱이 필요합니다.

자세한 조작법과 변환 지원 범위는 [사용 설명서](사용방법.txt)에 있습니다.

## 소스에서 빌드

Windows x64에서 Rust와 해당 도구 체인의 C/C++ 링커를 준비한 뒤 PowerShell에서 실행하세요. 렌더링에 사용하는 PDFium DLL과 라이선스는 저장소에 포함돼 있습니다.

```powershell
.\build.ps1
.\target\release\SimplePDF.exe --self-test .\target\self-test
```

설치 파일을 만들려면 Inno Setup 6의 컴파일러 경로를 지정하세요.

```powershell
.\build.ps1 -Package -InnoCompiler "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"
```

빌드한 설치 파일은 `installers` 폴더에 생성됩니다.

## 라이선스

SimplePDF는 [AGPL-3.0-or-later](LICENSE)로 배포합니다. PDFium과 구성 요소의 라이선스는 [PDFium 라이선스 폴더](native/pdfium/BUILD_LICENSES)에 있습니다.
