package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
)

const (
	accessKeyPath = "/run/secrets/object_access_key"
	secretKeyPath = "/run/secrets/object_secret_key"
)

type aliasCredentials struct {
	URL       string `json:"url"`
	AccessKey string `json:"accessKey"`
	SecretKey string `json:"secretKey"`
	API       string `json:"api"`
	Path      string `json:"path"`
}

func readSecret(path string) (string, error) {
	value, err := os.ReadFile(path)
	if err != nil {
		return "", errors.New("credential file could not be read")
	}
	if bytes.HasSuffix(value, []byte{'\n'}) {
		value = value[:len(value)-1]
		if bytes.HasSuffix(value, []byte{'\r'}) {
			value = value[:len(value)-1]
		}
	}
	if len(value) == 0 || bytes.ContainsAny(value, "\r\n\x00") {
		return "", errors.New("credential file is empty or multiline")
	}
	return string(value), nil
}

func run() error {
	if len(os.Args) != 2 {
		return errors.New("usage: mc-credentials ENDPOINT")
	}
	accessKey, err := readSecret(accessKeyPath)
	if err != nil {
		return fmt.Errorf("object access key: %w", err)
	}
	secretKey, err := readSecret(secretKeyPath)
	if err != nil {
		return fmt.Errorf("object secret key: %w", err)
	}
	return json.NewEncoder(os.Stdout).Encode(aliasCredentials{
		URL:       os.Args[1],
		AccessKey: accessKey,
		SecretKey: secretKey,
		API:       "s3v4",
		Path:      "auto",
	})
}

func main() {
	if err := run(); err != nil {
		fmt.Fprintf(os.Stderr, "mc credential configuration failed: %v\n", err)
		os.Exit(1)
	}
}
